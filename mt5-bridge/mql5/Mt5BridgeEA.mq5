//+------------------------------------------------------------------+
//|                                                   Mt5BridgeEA.mq5 |
//|          Deriv MT5 DEMO execution bridge — terminal-side expert   |
//+------------------------------------------------------------------+
//  This EA is the *client* of the bridge's loopback TCP listener, because
//  MQL5's built-in socket API can only create client sockets
//  (SocketCreate/SocketConnect) and has no SocketBind/SocketListen.
//
//  It is deliberately dumb and defensive:
//    * it executes only what the bridge asks for, over an authenticated
//      loopback session (InpToken must match the bridge's MT5_EA_TOKEN);
//    * it refuses every write method unless the terminal is logged into a
//      DEMO account (second layer after the bridge's own demo guard);
//    * it never decides position size, stops or direction.
//
//  Wire format (see mt5-bridge/src/proto.rs):
//    EA     -> bridge : HELLO token=.. build=.. login=.. server=.. mode=demo company=.. currency=USD ea=1.0.0
//    bridge -> EA     : HELLOOK proto=1
//    bridge -> EA     : REQ <id> <METHOD> k=v k=v ...
//    EA     -> bridge : RESP <id> OK <k=v ...>
//                       ITEM <id> <k=v ...>   (repeated)
//                       END  <id> count=N
//                       RESP <id> ERR code=<int> msg=<text>
//                       HB mode=demo connected=1 trade_allowed=1 login=.. ts=..
//                       EVT TRADE kind=.. position=.. deal=.. symbol=.. volume=.. price=.. profit=..
//  Values are percent-encoded (space, %, =, tab, CR, LF) so they can never
//  break tokenisation.
//
//  Install: MQL5/Experts/Mt5BridgeEA.mq5, allow algorithmic trading, set
//  InpToken to the same value as the bridge's MT5_EA_TOKEN.
//+------------------------------------------------------------------+
#property copyright "XAUUSD Node 3"
#property version   "1.0.0"
#property description "Executes Deriv MT5 demo orders requested by mt5-bridge over loopback TCP."

input string InpBridgeHost     = "127.0.0.1";  // Bridge host (loopback)
input int    InpBridgePort     = 5055;         // Bridge EA port (MT5_EA_PORT)
input string InpToken          = "";           // Shared token (must equal MT5_EA_TOKEN)
input string InpAllowedSymbols = "XAUUSD,XAUUSD.a,GOLD"; // Allowed symbols (comma separated, empty = any)
input int    InpHeartbeatSecs  = 5;            // Heartbeat interval (seconds)
input int    InpMagicFilter    = 0;            // Only manage positions with this magic (0 = bridge decides)
input bool   InpVerbose        = false;        // Verbose logging

//--- connection state -------------------------------------------------
int    gSocket        = INVALID_HANDLE;
string gRxBuffer      = "";
bool   gHelloSent     = false;
bool   gTokenRejected = false;
datetime gLastConnectAttempt = 0;
datetime gLastHeartbeat      = 0;
int    gReconnectDelaySecs   = 2;

#define EA_VERSION "1.0.0"

//+------------------------------------------------------------------+
//| Percent codec                                                    |
//+------------------------------------------------------------------+
string Enc(const string value)
{
   string out = "";
   int n = StringLen(value);
   for(int i = 0; i < n; i++)
   {
      ushort c = StringGetCharacter(value, i);
      if(c == ' ')       out += "%20";
      else if(c == '%')  out += "%25";
      else if(c == '=')  out += "%3D";
      else if(c == '\t') out += "%09";
      else if(c == '\r') out += "%0D";
      else if(c == '\n') out += "%0A";
      else               out += ShortToString(c);
   }
   return out;
}

int HexVal(const ushort c)
{
   if(c >= '0' && c <= '9') return (int)(c - '0');
   if(c >= 'a' && c <= 'f') return (int)(c - 'a') + 10;
   if(c >= 'A' && c <= 'F') return (int)(c - 'A') + 10;
   return -1;
}

string Dec(const string value)
{
   string out = "";
   int n = StringLen(value);
   int i = 0;
   while(i < n)
   {
      ushort c = StringGetCharacter(value, i);
      if(c == '%' && i + 2 < n)
      {
         int hi = HexVal(StringGetCharacter(value, i + 1));
         int lo = HexVal(StringGetCharacter(value, i + 2));
         if(hi >= 0 && lo >= 0)
         {
            out += CharToString((uchar)(hi * 16 + lo));
            i += 3;
            continue;
         }
      }
      out += ShortToString(c);
      i++;
   }
   return out;
}

//--- parameter parsing ------------------------------------------------
int ParseParams(const string &tokens[], const int start, string &keys[], string &values[])
{
   int count = ArraySize(tokens);
   int found = 0;
   ArrayResize(keys, count);
   ArrayResize(values, count);
   for(int i = start; i < count; i++)
   {
      string token = tokens[i];
      if(StringLen(token) == 0) continue;
      int eq = StringFind(token, "=");
      if(eq < 0)
      {
         keys[found]   = token;
         values[found] = "";
      }
      else
      {
         keys[found]   = StringSubstr(token, 0, eq);
         values[found] = Dec(StringSubstr(token, eq + 1));
      }
      found++;
   }
   return found;
}

string Param(const string &keys[], const string &values[], const int count, const string key, const string fallback = "")
{
   for(int i = 0; i < count; i++)
      if(keys[i] == key) return values[i];
   return fallback;
}

bool HasParam(const string &keys[], const string &values[], const int count, const string key)
{
   for(int i = 0; i < count; i++)
      if(keys[i] == key) return true;
   return false;
}

//--- socket helpers ---------------------------------------------------
bool SendLine(const string line)
{
   if(gSocket == INVALID_HANDLE) return false;
   uchar buffer[];
   string payload = line + "\n";
   int length = StringToCharArray(payload, buffer, 0, WHOLE_ARRAY, CP_UTF8) - 1;
   if(length <= 0) return false;
   // SocketSend() always writes from the start of the array, so a partial write
   // must be resumed with a copy of the unsent remainder. Resending the whole
   // buffer would duplicate the line and corrupt the stream.
   int sent = 0;
   while(sent < length)
   {
      uchar chunk[];
      int remaining = length - sent;
      ArrayResize(chunk, remaining);
      ArrayCopy(chunk, buffer, 0, sent, remaining);
      int written = SocketSend(gSocket, chunk, remaining);
      if(written <= 0)
      {
         if(InpVerbose) PrintFormat("Mt5BridgeEA: SocketSend failed (%d)", GetLastError());
         return false;
      }
      sent += written;
   }
   return true;
}

//--- account / symbol helpers ----------------------------------------
string AccountMode()
{
   long mode = AccountInfoInteger(ACCOUNT_TRADE_MODE);
   if(mode == ACCOUNT_TRADE_MODE_DEMO)    return "demo";
   if(mode == ACCOUNT_TRADE_MODE_CONTEST) return "contest";
   if(mode == ACCOUNT_TRADE_MODE_REAL)    return "real";
   return "unknown";
}

bool IsDemoAccount()
{
   return AccountInfoInteger(ACCOUNT_TRADE_MODE) == ACCOUNT_TRADE_MODE_DEMO;
}

bool SymbolAllowed(const string symbol)
{
   if(StringLen(InpAllowedSymbols) == 0) return true;
   string parts[];
   int count = StringSplit(InpAllowedSymbols, ',', parts);
   for(int i = 0; i < count; i++)
   {
      string candidate = parts[i];
      StringTrimLeft(candidate);
      StringTrimRight(candidate);
      if(candidate == symbol) return true;
   }
   return false;
}

double NormalizeVolume(const string symbol, const double requested)
{
   double min_volume  = SymbolInfoDouble(symbol, SYMBOL_VOLUME_MIN);
   double max_volume  = SymbolInfoDouble(symbol, SYMBOL_VOLUME_MAX);
   double step        = SymbolInfoDouble(symbol, SYMBOL_VOLUME_STEP);
   if(step <= 0.0) step = 0.01;
   double volume = MathFloor(requested / step + 0.0000001) * step;
   volume = NormalizeDouble(volume, 8);
   if(volume < min_volume) volume = min_volume;
   if(max_volume > 0.0 && volume > max_volume) volume = max_volume;
   return volume;
}

ENUM_ORDER_TYPE_FILLING PickFilling(const string symbol)
{
   long filling = SymbolInfoInteger(symbol, SYMBOL_FILLING_MODE);
   if((filling & SYMBOL_FILLING_FOK) != 0) return ORDER_FILLING_FOK;
   if((filling & SYMBOL_FILLING_IOC) != 0) return ORDER_FILLING_IOC;
   return ORDER_FILLING_RETURN;
}

string RetcodeDesc(const uint retcode)
{
   switch(retcode)
   {
      case TRADE_RETCODE_REQUOTE:        return "requote";
      case TRADE_RETCODE_REJECT:         return "rejected";
      case TRADE_RETCODE_CANCEL:         return "cancelled by trader";
      case TRADE_RETCODE_PLACED:         return "order placed";
      case TRADE_RETCODE_DONE:           return "done";
      case TRADE_RETCODE_DONE_PARTIAL:   return "partially filled";
      case TRADE_RETCODE_ERROR:          return "request processing error";
      case TRADE_RETCODE_TIMEOUT:        return "request timed out";
      case TRADE_RETCODE_INVALID:        return "invalid request";
      case TRADE_RETCODE_INVALID_VOLUME: return "invalid volume";
      case TRADE_RETCODE_INVALID_PRICE:  return "invalid price";
      case TRADE_RETCODE_INVALID_STOPS:  return "invalid stops";
      case TRADE_RETCODE_TRADE_DISABLED: return "trading disabled";
      case TRADE_RETCODE_MARKET_CLOSED:  return "market closed";
      case TRADE_RETCODE_NO_MONEY:       return "not enough money";
      case TRADE_RETCODE_PRICE_CHANGED:  return "price changed";
      case TRADE_RETCODE_PRICE_OFF:      return "no quotes to process";
      case TRADE_RETCODE_TOO_MANY_REQUESTS: return "too many requests";
      case TRADE_RETCODE_LOCKED:         return "request locked";
      case TRADE_RETCODE_FROZEN:         return "order or position frozen";
      case TRADE_RETCODE_CONNECTION:     return "no connection to trade server";
      case TRADE_RETCODE_LIMIT_VOLUME:   return "volume limit reached";
      case TRADE_RETCODE_INVALID_ORDER:  return "invalid order";
      case TRADE_RETCODE_POSITION_CLOSED: return "position already closed";
      case TRADE_RETCODE_INVALID_CLOSE_VOLUME: return "invalid close volume";
      case TRADE_RETCODE_CLOSE_ORDER_EXIST: return "close order already exists";
      case TRADE_RETCODE_LIMIT_POSITIONS: return "position limit reached";
      case TRADE_RETCODE_INVALID_FILL:   return "unsupported filling mode";
      case TRADE_RETCODE_ONLY_REAL:      return "operation allowed on live accounts only";
      case TRADE_RETCODE_INVALID_SYMBOL: return "invalid symbol";
      default:                           return "retcode " + IntegerToString((int)retcode);
   }
}

string OutcomeStatus(const uint retcode)
{
   if(retcode == TRADE_RETCODE_DONE || retcode == TRADE_RETCODE_PLACED) return "filled";
   if(retcode == TRADE_RETCODE_DONE_PARTIAL) return "partial";
   return "rejected";
}

ulong PositionIdFromDeal(const ulong deal_ticket)
{
   if(deal_ticket == 0) return 0;
   if(!HistoryDealSelect(deal_ticket)) return 0;
   return (ulong)HistoryDealGetInteger(deal_ticket, DEAL_POSITION_ID);
}

//--- row builders -----------------------------------------------------
string PositionRow(const ulong ticket)
{
   if(!PositionSelectByTicket(ticket)) return "";
   string symbol   = PositionGetString(POSITION_SYMBOL);
   long   type     = PositionGetInteger(POSITION_TYPE);
   string side     = (type == POSITION_TYPE_BUY) ? "buy" : "sell";
   double volume   = PositionGetDouble(POSITION_VOLUME);
   double open     = PositionGetDouble(POSITION_PRICE_OPEN);
   double sl       = PositionGetDouble(POSITION_SL);
   double tp       = PositionGetDouble(POSITION_TP);
   double profit   = PositionGetDouble(POSITION_PROFIT);
   double swap     = PositionGetDouble(POSITION_SWAP);
   string comment  = PositionGetString(POSITION_COMMENT);
   long   magic    = PositionGetInteger(POSITION_MAGIC);
   long   time_msc = PositionGetInteger(POSITION_TIME_MSC);
   double current  = (type == POSITION_TYPE_BUY) ? SymbolInfoDouble(symbol, SYMBOL_BID)
                                                 : SymbolInfoDouble(symbol, SYMBOL_ASK);
   return StringFormat(
      "kind=position ticket=%I64u symbol=%s side=%s volume=%s price_open=%s sl=%s tp=%s "
      "profit=%s swap=%s comment=%s magic=%I64d time=%I64d current_price=%s unrealized_pnl=%s",
      ticket, Enc(symbol), side, DoubleToString(volume, 8), DoubleToString(open, 8),
      DoubleToString(sl, 8), DoubleToString(tp, 8), DoubleToString(profit, 8),
      DoubleToString(swap, 8), Enc(comment), magic, time_msc,
      DoubleToString(current, 8), DoubleToString(profit, 8));
}

string OrderRow(const ulong ticket)
{
   if(!OrderSelect(ticket)) return "";
   string symbol  = OrderGetString(ORDER_SYMBOL);
   long   type    = OrderGetInteger(ORDER_TYPE);
   double volume  = OrderGetDouble(ORDER_VOLUME_CURRENT);
   double price   = OrderGetDouble(ORDER_PRICE_OPEN);
   double sl      = OrderGetDouble(ORDER_SL);
   double tp      = OrderGetDouble(ORDER_TP);
   string comment = OrderGetString(ORDER_COMMENT);
   long   magic   = OrderGetInteger(ORDER_MAGIC);
   long   time_msc = OrderGetInteger(ORDER_TIME_SETUP_MSC);
   return StringFormat(
      "kind=order ticket=%I64u symbol=%s type=%I64d volume=%s price=%s sl=%s tp=%s comment=%s magic=%I64d time=%I64d",
      ticket, Enc(symbol), type, DoubleToString(volume, 8), DoubleToString(price, 8),
      DoubleToString(sl, 8), DoubleToString(tp, 8), Enc(comment), magic, time_msc);
}

string DealEntryName(const long entry)
{
   if(entry == DEAL_ENTRY_IN)    return "in";
   if(entry == DEAL_ENTRY_OUT)   return "out";
   if(entry == DEAL_ENTRY_INOUT) return "inout";
   if(entry == DEAL_ENTRY_OUT_BY) return "out_by";
   return "unknown";
}

string DealReasonName(const long reason)
{
   switch((ENUM_DEAL_REASON)reason)
   {
      case DEAL_REASON_CLIENT:   return "client";
      case DEAL_REASON_MOBILE:   return "mobile";
      case DEAL_REASON_WEB:      return "web";
      case DEAL_REASON_EXPERT:   return "expert";
      case DEAL_REASON_SL:       return "sl";
      case DEAL_REASON_TP:       return "tp";
      case DEAL_REASON_SO:       return "so";
      case DEAL_REASON_ROLLOVER: return "rollover";
      case DEAL_REASON_VMARGIN:  return "vmargin";
      case DEAL_REASON_SPLIT:    return "split";
   }
   return "reason_" + IntegerToString(reason);
}

string DealRow(const ulong ticket)
{
   if(!HistoryDealSelect(ticket)) return "";
   string symbol  = HistoryDealGetString(ticket, DEAL_SYMBOL);
   long   type    = HistoryDealGetInteger(ticket, DEAL_TYPE);
   double volume  = HistoryDealGetDouble(ticket, DEAL_VOLUME);
   double price   = HistoryDealGetDouble(ticket, DEAL_PRICE);
   double profit  = HistoryDealGetDouble(ticket, DEAL_PROFIT);
   double swap    = HistoryDealGetDouble(ticket, DEAL_SWAP);
   double commission = HistoryDealGetDouble(ticket, DEAL_COMMISSION);
   string comment = HistoryDealGetString(ticket, DEAL_COMMENT);
   long   magic   = HistoryDealGetInteger(ticket, DEAL_MAGIC);
   long   time_msc = HistoryDealGetInteger(ticket, DEAL_TIME_MSC);
   long   entry   = HistoryDealGetInteger(ticket, DEAL_ENTRY);
   long   reason  = HistoryDealGetInteger(ticket, DEAL_REASON);
   long   order   = HistoryDealGetInteger(ticket, DEAL_ORDER);
   long   position = HistoryDealGetInteger(ticket, DEAL_POSITION_ID);
   string side = (type == DEAL_TYPE_BUY) ? "buy" : "sell";
   return StringFormat(
      "kind=deal ticket=%I64u order=%I64d position=%I64d symbol=%s side=%s volume=%s price=%s "
      "profit=%s swap=%s commission=%s comment=%s magic=%I64d time=%I64d "
      "entry=%I64d entry_name=%s reason=%I64d reason_name=%s",
      ticket, order, position, Enc(symbol), side, DoubleToString(volume, 8),
      DoubleToString(price, 8), DoubleToString(profit, 8), DoubleToString(swap, 8),
      DoubleToString(commission, 8), Enc(comment), magic, time_msc, entry,
      DealEntryName(entry), reason, DealReasonName(reason));
}

//--- replies ----------------------------------------------------------
void ReplyOk(const string id, const string body)
{
   string line = "RESP " + id + " OK";
   if(StringLen(body) > 0) line += " " + body;
   SendLine(line);
}

void ReplyErr(const string id, const int code, const string message)
{
   SendLine(StringFormat("RESP %s ERR code=%d msg=%s", id, code, Enc(message)));
}

void ReplyList(const string id, const string rows[])
{
   int count = ArraySize(rows);
   SendLine(StringFormat("RESP %s OK count=%d", id, count));
   for(int i = 0; i < count; i++)
      SendLine("ITEM " + id + " " + rows[i]);
   SendLine(StringFormat("END %s count=%d", id, count));
}

//+------------------------------------------------------------------+
//| Request handlers                                                  |
//+------------------------------------------------------------------+
void HandleAccount(const string id, const string &keys[], const string &values[], const int count)
{
   ReplyOk(id, StringFormat(
      "login=%I64d server=%s company=%s mode=%s currency=%s balance=%s equity=%s margin=%s "
      "margin_free=%s leverage=%I64d trade_allowed=%d connected=%d build=%I64d",
      AccountInfoInteger(ACCOUNT_LOGIN),
      Enc(AccountInfoString(ACCOUNT_SERVER)),
      Enc(AccountInfoString(ACCOUNT_COMPANY)),
      AccountMode(),
      Enc(AccountInfoString(ACCOUNT_CURRENCY)),
      DoubleToString(AccountInfoDouble(ACCOUNT_BALANCE), 8),
      DoubleToString(AccountInfoDouble(ACCOUNT_EQUITY), 8),
      DoubleToString(AccountInfoDouble(ACCOUNT_MARGIN), 8),
      DoubleToString(AccountInfoDouble(ACCOUNT_MARGIN_FREE), 8),
      AccountInfoInteger(ACCOUNT_LEVERAGE),
      AccountInfoInteger(ACCOUNT_TRADE_ALLOWED) ? 1 : 0,
      (TerminalInfoInteger(TERMINAL_CONNECTED) ? 1 : 0),
      TerminalInfoInteger(TERMINAL_BUILD)));
}

void HandleSymbol(const string id, const string &keys[], const string &values[], const int count)
{
   string symbol = Param(keys, values, count, "symbol");
   if(StringLen(symbol) == 0)
   {
      ReplyErr(id, 4003, "symbol is required");
      return;
   }
   if(!SymbolSelect(symbol, true))
   {
      ReplyErr(id, 43001, "unknown symbol " + symbol);
      return;
   }
   MqlTick tick;
   SymbolInfoTick(symbol, tick);
   long filling = SymbolInfoInteger(symbol, SYMBOL_FILLING_MODE);
   ReplyOk(id, StringFormat(
      "name=%s digits=%d point=%s tick_size=%s tick_value=%s contract_size=%s volume_min=%s "
      "volume_max=%s volume_step=%s trade_mode=%I64d stops_level=%I64d freeze_level=%I64d "
      "bid=%s ask=%s spread_points=%s filling=%I64d ts=%I64d",
      Enc(symbol),
      (int)SymbolInfoInteger(symbol, SYMBOL_DIGITS),
      DoubleToString(SymbolInfoDouble(symbol, SYMBOL_POINT), 8),
      DoubleToString(SymbolInfoDouble(symbol, SYMBOL_TRADE_TICK_SIZE), 8),
      DoubleToString(SymbolInfoDouble(symbol, SYMBOL_TRADE_TICK_VALUE), 8),
      DoubleToString(SymbolInfoDouble(symbol, SYMBOL_TRADE_CONTRACT_SIZE), 8),
      DoubleToString(SymbolInfoDouble(symbol, SYMBOL_VOLUME_MIN), 8),
      DoubleToString(SymbolInfoDouble(symbol, SYMBOL_VOLUME_MAX), 8),
      DoubleToString(SymbolInfoDouble(symbol, SYMBOL_VOLUME_STEP), 8),
      SymbolInfoInteger(symbol, SYMBOL_TRADE_MODE),
      SymbolInfoInteger(symbol, SYMBOL_TRADE_STOPS_LEVEL),
      SymbolInfoInteger(symbol, SYMBOL_TRADE_FREEZE_LEVEL),
      DoubleToString(tick.bid, 8),
      DoubleToString(tick.ask, 8),
      DoubleToString(SymbolInfoDouble(symbol, SYMBOL_SPREAD), 8),
      filling,
      tick.time_msc));
}

void HandleQuote(const string id, const string &keys[], const string &values[], const int count)
{
   string symbol = Param(keys, values, count, "symbol");
   if(!SymbolSelect(symbol, true))
   {
      ReplyErr(id, 43001, "unknown symbol " + symbol);
      return;
   }
   MqlTick tick;
   if(!SymbolInfoTick(symbol, tick))
   {
      ReplyErr(id, 4107, "no tick available");
      return;
   }
   ReplyOk(id, StringFormat("bid=%s ask=%s ts=%I64d",
      DoubleToString(tick.bid, 8), DoubleToString(tick.ask, 8), tick.time_msc));
}

void HandlePositions(const string id, const string &keys[], const string &values[], const int count)
{
   string filter = Param(keys, values, count, "symbol");
   string rows[];
   int total = PositionsTotal();
   for(int i = 0; i < total; i++)
   {
      ulong ticket = PositionGetTicket(i);
      if(ticket == 0) continue;
      if(!PositionSelectByTicket(ticket)) continue;
      if(StringLen(filter) > 0 && PositionGetString(POSITION_SYMBOL) != filter) continue;
      ArrayResize(rows, ArraySize(rows) + 1);
      rows[ArraySize(rows) - 1] = PositionRow(ticket);
   }
   ReplyList(id, rows);
}

void HandleOrders(const string id, const string &keys[], const string &values[], const int count)
{
   string rows[];
   int total = OrdersTotal();
   for(int i = 0; i < total; i++)
   {
      ulong ticket = OrderGetTicket(i);
      if(ticket == 0) continue;
      string row = OrderRow(ticket);
      if(StringLen(row) == 0) continue;
      ArrayResize(rows, ArraySize(rows) + 1);
      rows[ArraySize(rows) - 1] = row;
   }
   ReplyList(id, rows);
}

void HandleHistory(const string id, const string &keys[], const string &values[], const int count)
{
   long from_ms = (long)StringToInteger(Param(keys, values, count, "from", "0"));
   long to_ms   = (long)StringToInteger(Param(keys, values, count, "to", "0"));
   int  limit   = (int)StringToInteger(Param(keys, values, count, "limit", "100"));
   if(to_ms <= 0) to_ms = (long)TimeCurrent() * 1000 + 60000;
   datetime from = (datetime)(from_ms / 1000);
   datetime to   = (datetime)(to_ms / 1000 + 1);
   if(!HistorySelect(from, to))
   {
      ReplyErr(id, 4401, "history select failed");
      return;
   }
   string rows[];
   int total = HistoryDealsTotal();
   for(int i = 0; i < total && ArraySize(rows) < limit; i++)
   {
      ulong ticket = HistoryDealGetTicket(i);
      if(ticket == 0) continue;
      string row = DealRow(ticket);
      if(StringLen(row) == 0) continue;
      ArrayResize(rows, ArraySize(rows) + 1);
      rows[ArraySize(rows) - 1] = row;
   }
   ReplyList(id, rows);
}

// Reconciliation lookup: everything that carries this comment/magic.
void HandleFind(const string id, const string &keys[], const string &values[], const int count)
{
   string comment = Param(keys, values, count, "comment");
   long   magic   = (long)StringToInteger(Param(keys, values, count, "magic", "0"));
   long   from_ms = (long)StringToInteger(Param(keys, values, count, "from", "0"));
   if(from_ms <= 0) from_ms = (long)TimeCurrent() * 1000 - 86400000;

   string rows[];
   int total_positions = PositionsTotal();
   for(int i = 0; i < total_positions; i++)
   {
      ulong ticket = PositionGetTicket(i);
      if(ticket == 0 || !PositionSelectByTicket(ticket)) continue;
      long position_magic = PositionGetInteger(POSITION_MAGIC);
      string position_comment = PositionGetString(POSITION_COMMENT);
      if(magic != 0 && position_magic != magic) continue;
      if(StringLen(comment) > 0 && StringFind(position_comment, comment) < 0) continue;
      ArrayResize(rows, ArraySize(rows) + 1);
      rows[ArraySize(rows) - 1] = PositionRow(ticket);
   }

   datetime from = (datetime)(from_ms / 1000);
   datetime to   = (datetime)((long)TimeCurrent() + 3600);
   if(HistorySelect(from, to))
   {
      int total_deals = HistoryDealsTotal();
      for(int i = 0; i < total_deals; i++)
      {
         ulong ticket = HistoryDealGetTicket(i);
         if(ticket == 0) continue;
         long deal_magic = HistoryDealGetInteger(ticket, DEAL_MAGIC);
         string deal_comment = HistoryDealGetString(ticket, DEAL_COMMENT);
         if(magic != 0 && deal_magic != magic) continue;
         if(StringLen(comment) > 0 && StringFind(deal_comment, comment) < 0) continue;
         string row = DealRow(ticket);
         if(StringLen(row) == 0) continue;
         ArrayResize(rows, ArraySize(rows) + 1);
         rows[ArraySize(rows) - 1] = row;
      }
   }
   ReplyList(id, rows);
}

void HandleOrderSend(const string id, const string &keys[], const string &values[], const int count)
{
   // Second demo layer: the terminal itself refuses non-demo writes, even if
   // the bridge asked for one.
   if(!IsDemoAccount())
   {
      ReplyErr(id, 10017, "refused: account mode is " + AccountMode() + ", demo only");
      return;
   }
   if(!TerminalInfoInteger(TERMINAL_TRADE_ALLOWED))
   {
      ReplyErr(id, 10017, "refused: algorithmic trading is disabled in the terminal");
      return;
   }

   string symbol = Param(keys, values, count, "symbol");
   string side   = Param(keys, values, count, "side");
   if(StringLen(symbol) == 0 || (side != "buy" && side != "sell"))
   {
      ReplyErr(id, 4003, "symbol and side (buy|sell) are required");
      return;
   }
   if(!SymbolAllowed(symbol))
   {
      ReplyErr(id, 10047, "symbol " + symbol + " is not in InpAllowedSymbols");
      return;
   }
   if(!SymbolSelect(symbol, true))
   {
      ReplyErr(id, 43001, "unknown symbol " + symbol);
      return;
   }
   if(SymbolInfoInteger(symbol, SYMBOL_TRADE_MODE) != SYMBOL_TRADE_MODE_FULL)
   {
      ReplyErr(id, 10017, "symbol is not fully tradable");
      return;
   }

   double volume = NormalizeVolume(symbol, StringToDouble(Param(keys, values, count, "volume", "0")));
   double sl     = StringToDouble(Param(keys, values, count, "sl", "0"));
   double tp     = StringToDouble(Param(keys, values, count, "tp", "0"));
   int deviation = (int)StringToInteger(Param(keys, values, count, "deviation", "20"));
   long magic    = (long)StringToInteger(Param(keys, values, count, "magic", "0"));
   // The comment is how an uncertain order is found again (FIND comment=...), so
   // it carries the bridge's intent id. MT5 truncates comments over 31 chars.
   string intent  = Param(keys, values, count, "intent");
   string comment = Param(keys, values, count, "comment");
   if(StringLen(comment) == 0) comment = intent;
   if(StringLen(comment) == 0) comment = "N3";
   if(StringLen(comment) > 31) comment = StringSubstr(comment, 0, 31);

   MqlTick tick;
   if(!SymbolInfoTick(symbol, tick))
   {
      ReplyErr(id, 4107, "no tick available");
      return;
   }

   MqlTradeRequest request;
   MqlTradeResult  result;
   ZeroMemory(request);
   ZeroMemory(result);
   request.action       = TRADE_ACTION_DEAL;
   request.symbol       = symbol;
   request.volume       = volume;
   request.type         = (side == "buy") ? ORDER_TYPE_BUY : ORDER_TYPE_SELL;
   request.price        = (side == "buy") ? tick.ask : tick.bid;
   request.sl           = sl;
   request.tp           = tp;
   request.deviation    = (ulong)deviation;
   request.magic        = (ulong)magic;
   request.comment      = comment;
   request.type_time    = ORDER_TIME_GTC;
   request.type_filling = PickFilling(symbol);

   bool sent = OrderSend(request, result);
   string status = OutcomeStatus(result.retcode);
   string body = StringFormat("status=%s retcode=%d retcode_desc=%s",
                              status, result.retcode, Enc(RetcodeDesc(result.retcode)));
   if(result.order > 0) body += StringFormat(" order=%I64u", result.order);
   if(result.deal > 0)
   {
      body += StringFormat(" deal=%I64u", result.deal);
      ulong position = PositionIdFromDeal(result.deal);
      if(position > 0) body += StringFormat(" position=%I64u", position);
   }
   if(result.volume > 0) body += " volume=" + DoubleToString(result.volume, 8);
   if(result.price > 0)  body += " price=" + DoubleToString(result.price, 8);
   body += " ts=" + IntegerToString((long)tick.time_msc);
   if(StringLen(intent) > 0) body += " intent=" + Enc(intent);
   body += " comment=" + Enc(comment);
   if(!sent && result.retcode == 0)
      body += " msg=" + Enc("OrderSend returned false, " + IntegerToString(GetLastError()));

   if(InpVerbose)
      PrintFormat("Mt5BridgeEA: ORDER_SEND %s %s %s -> retcode %d (%s)",
                  side, DoubleToString(volume, 8), symbol, result.retcode, status);

   ReplyOk(id, body);
}

void HandlePositionModify(const string id, const string &keys[], const string &values[], const int count)
{
   if(!IsDemoAccount())
   {
      ReplyErr(id, 10017, "refused: demo accounts only");
      return;
   }
   ulong ticket = (ulong)StringToInteger(Param(keys, values, count, "ticket", "0"));
   double sl = StringToDouble(Param(keys, values, count, "sl", "0"));
   double tp = StringToDouble(Param(keys, values, count, "tp", "0"));
   if(ticket == 0 || !PositionSelectByTicket(ticket))
   {
      ReplyErr(id, 10013, "position not found");
      return;
   }
   string symbol = PositionGetString(POSITION_SYMBOL);
   MqlTradeRequest request;
   MqlTradeResult  result;
   ZeroMemory(request);
   ZeroMemory(result);
   request.action   = TRADE_ACTION_SLTP;
   request.symbol   = symbol;
   request.position = ticket;
   request.sl       = sl;
   request.tp       = tp;
   bool sent = OrderSend(request, result);
   if(!sent || (result.retcode != TRADE_RETCODE_DONE && result.retcode != TRADE_RETCODE_PLACED))
   {
      ReplyErr(id, (int)result.retcode, RetcodeDesc(result.retcode));
      return;
   }
   ReplyOk(id, StringFormat("retcode=%d sl=%s tp=%s", result.retcode,
                            DoubleToString(sl, 8), DoubleToString(tp, 8)));
}

void HandlePositionClose(const string id, const string &keys[], const string &values[], const int count)
{
   if(!IsDemoAccount())
   {
      ReplyErr(id, 10017, "refused: demo accounts only");
      return;
   }
   ulong ticket = (ulong)StringToInteger(Param(keys, values, count, "ticket", "0"));
   double volume = StringToDouble(Param(keys, values, count, "volume", "0"));
   int deviation = (int)StringToInteger(Param(keys, values, count, "deviation", "20"));
   if(ticket == 0 || !PositionSelectByTicket(ticket))
   {
      ReplyErr(id, 10013, "position not found");
      return;
   }
   string symbol = PositionGetString(POSITION_SYMBOL);
   long type = PositionGetInteger(POSITION_TYPE);
   double position_volume = PositionGetDouble(POSITION_VOLUME);
   if(volume <= 0.0 || volume > position_volume) volume = position_volume;
   volume = NormalizeVolume(symbol, volume);

   MqlTick tick;
   if(!SymbolInfoTick(symbol, tick))
   {
      ReplyErr(id, 4107, "no tick available");
      return;
   }
   MqlTradeRequest request;
   MqlTradeResult  result;
   ZeroMemory(request);
   ZeroMemory(result);
   request.action       = TRADE_ACTION_DEAL;
   request.symbol       = symbol;
   request.position     = ticket;
   request.volume       = volume;
   request.deviation    = (ulong)deviation;
   request.type         = (type == POSITION_TYPE_BUY) ? ORDER_TYPE_SELL : ORDER_TYPE_BUY;
   request.price        = (type == POSITION_TYPE_BUY) ? tick.bid : tick.ask;
   request.type_time    = ORDER_TIME_GTC;
   request.type_filling = PickFilling(symbol);
   bool sent = OrderSend(request, result);
   if(!sent && result.retcode == 0)
   {
      ReplyErr(id, 4109, "OrderSend failed, " + IntegerToString(GetLastError()));
      return;
   }
   string body = StringFormat("status=%s retcode=%d retcode_desc=%s position=%I64u",
                              OutcomeStatus(result.retcode), result.retcode,
                              Enc(RetcodeDesc(result.retcode)), ticket);
   if(result.order > 0) body += StringFormat(" order=%I64u", result.order);
   if(result.deal > 0)  body += StringFormat(" deal=%I64u", result.deal);
   if(result.volume > 0) body += " volume=" + DoubleToString(result.volume, 8);
   if(result.price > 0)  body += " price=" + DoubleToString(result.price, 8);
   ReplyOk(id, body);
}

void HandleCloseAll(const string id, const string &keys[], const string &values[], const int count)
{
   if(!IsDemoAccount())
   {
      ReplyErr(id, 10017, "refused: demo accounts only");
      return;
   }
   long magic = (long)StringToInteger(Param(keys, values, count, "magic", "0"));
   string symbol_filter = Param(keys, values, count, "symbol");
   int deviation = (int)StringToInteger(Param(keys, values, count, "deviation", "20"));

   string rows[];
   int total = PositionsTotal();
   for(int i = total - 1; i >= 0; i--)
   {
      ulong ticket = PositionGetTicket(i);
      if(ticket == 0 || !PositionSelectByTicket(ticket)) continue;
      if(magic != 0 && (long)PositionGetInteger(POSITION_MAGIC) != magic) continue;
      if(StringLen(symbol_filter) > 0 && PositionGetString(POSITION_SYMBOL) != symbol_filter) continue;
      string symbol = PositionGetString(POSITION_SYMBOL);
      long type = PositionGetInteger(POSITION_TYPE);
      double volume = PositionGetDouble(POSITION_VOLUME);
      MqlTick tick;
      if(!SymbolInfoTick(symbol, tick))
      {
         ArrayResize(rows, ArraySize(rows) + 1);
         rows[ArraySize(rows) - 1] = StringFormat("position=%I64u ok=0 error=%s", ticket, Enc("no tick"));
         continue;
      }
      MqlTradeRequest request;
      MqlTradeResult  result;
      ZeroMemory(request);
      ZeroMemory(result);
      request.action       = TRADE_ACTION_DEAL;
      request.symbol       = symbol;
      request.position     = ticket;
      request.volume       = volume;
      request.deviation    = (ulong)deviation;
      request.type         = (type == POSITION_TYPE_BUY) ? ORDER_TYPE_SELL : ORDER_TYPE_BUY;
      request.price        = (type == POSITION_TYPE_BUY) ? tick.bid : tick.ask;
      request.type_time    = ORDER_TIME_GTC;
      request.type_filling = PickFilling(symbol);
      OrderSend(request, result);
      bool ok = (result.retcode == TRADE_RETCODE_DONE || result.retcode == TRADE_RETCODE_PLACED);
      ArrayResize(rows, ArraySize(rows) + 1);
      rows[ArraySize(rows) - 1] = StringFormat(
         "position=%I64u ok=%d volume=%s price=%s error=%s",
         ticket, ok ? 1 : 0, DoubleToString(result.volume, 8),
         DoubleToString(result.price, 8),
         Enc(ok ? "" : RetcodeDesc(result.retcode)));
   }
   ReplyList(id, rows);
}

void HandleOrderCancel(const string id, const string &keys[], const string &values[], const int count)
{
   if(!IsDemoAccount())
   {
      ReplyErr(id, 10017, "refused: demo accounts only");
      return;
   }
   ulong ticket = (ulong)StringToInteger(Param(keys, values, count, "ticket", "0"));
   if(ticket == 0)
   {
      ReplyErr(id, 4003, "ticket is required");
      return;
   }
   MqlTradeRequest request;
   MqlTradeResult  result;
   ZeroMemory(request);
   ZeroMemory(result);
   request.action = TRADE_ACTION_REMOVE;
   request.order  = ticket;
   bool sent = OrderSend(request, result);
   if(!sent || result.retcode != TRADE_RETCODE_DONE)
   {
      ReplyErr(id, (int)result.retcode, RetcodeDesc(result.retcode));
      return;
   }
   ReplyOk(id, StringFormat("retcode=%d", result.retcode));
}

//+------------------------------------------------------------------+
//| Frame dispatch                                                   |
//+------------------------------------------------------------------+
void ProcessLine(const string line)
{
   string tokens[];
   int count = StringSplit(line, ' ', tokens);
   if(count < 1) return;
   string head = tokens[0];
   StringToUpper(head);

   if(head == "REQ")
   {
      if(count < 3)
      {
         Print("Mt5BridgeEA: malformed REQ line");
         return;
      }
      string id = tokens[1];
      string method = tokens[2];
      StringToUpper(method);
      string keys[], values[];
      int params = ParseParams(tokens, 3, keys, values);

      if(method == "PING")             { ReplyOk(id, StringFormat("ts=%I64d", (long)TimeCurrent() * 1000)); return; }
      if(method == "ACCOUNT")          { HandleAccount(id, keys, values, params); return; }
      if(method == "SYMBOL")           { HandleSymbol(id, keys, values, params); return; }
      if(method == "QUOTE")            { HandleQuote(id, keys, values, params); return; }
      if(method == "POSITIONS")        { HandlePositions(id, keys, values, params); return; }
      if(method == "ORDERS")           { HandleOrders(id, keys, values, params); return; }
      if(method == "HISTORY")          { HandleHistory(id, keys, values, params); return; }
      if(method == "FIND")             { HandleFind(id, keys, values, params); return; }
      if(method == "ORDER_SEND")       { HandleOrderSend(id, keys, values, params); return; }
      if(method == "POS_MODIFY")       { HandlePositionModify(id, keys, values, params); return; }
      if(method == "POS_CLOSE")        { HandlePositionClose(id, keys, values, params); return; }
      if(method == "CLOSE_ALL")        { HandleCloseAll(id, keys, values, params); return; }
      if(method == "ORDER_CANCEL")     { HandleOrderCancel(id, keys, values, params); return; }
      ReplyErr(id, 4004, "unknown method " + method);
      return;
   }

   if(head == "HELLOOK")
   {
      Print("Mt5BridgeEA: bridge accepted the session (" + line + ")");
      return;
   }
   if(head == "HELLOERR")
   {
      gTokenRejected = true;
      Print("Mt5BridgeEA: bridge REJECTED the session — check that InpToken equals MT5_EA_TOKEN. " +
            "Disconnecting; fix the token and the EA will retry.");
      if(gSocket != INVALID_HANDLE)
      {
         SocketClose(gSocket);
         gSocket = INVALID_HANDLE;
      }
      return;
   }
   if(InpVerbose) Print("Mt5BridgeEA: ignoring line: " + line);
}

void DrainSocket()
{
   if(gSocket == INVALID_HANDLE) return;
   uchar buffer[];
   ArrayResize(buffer, 4096);
   int guard = 0;
   while(guard++ < 64)
   {
      int bytes = SocketRead(gSocket, buffer, ArraySize(buffer), 10);
      if(bytes <= 0) break;
      gRxBuffer += CharArrayToString(buffer, 0, bytes, CP_UTF8);
      // Extract complete lines.
      int newline = StringFind(gRxBuffer, "\n");
      while(newline >= 0)
      {
         string line = StringSubstr(gRxBuffer, 0, newline);
         gRxBuffer = StringSubstr(gRxBuffer, newline + 1);
         StringReplace(line, "\r", "");
         if(StringLen(line) > 0) ProcessLine(line);
         newline = StringFind(gRxBuffer, "\n");
      }
      if(bytes < ArraySize(buffer)) break;
   }
}

void SendHello()
{
   SendLine(StringFormat(
      "HELLO token=%s build=%I64d login=%I64d server=%s mode=%s company=%s currency=%s ea=%s",
      Enc(InpToken),
      TerminalInfoInteger(TERMINAL_BUILD),
      AccountInfoInteger(ACCOUNT_LOGIN),
      Enc(AccountInfoString(ACCOUNT_SERVER)),
      AccountMode(),
      Enc(AccountInfoString(ACCOUNT_COMPANY)),
      Enc(AccountInfoString(ACCOUNT_CURRENCY)),
      EA_VERSION));
   gHelloSent = true;
}

void SendHeartbeat()
{
   SendLine(StringFormat(
      "HB mode=%s connected=%d trade_allowed=%d login=%I64d ts=%I64d",
      AccountMode(),
      (TerminalInfoInteger(TERMINAL_CONNECTED) ? 1 : 0),
      (AccountInfoInteger(ACCOUNT_TRADE_ALLOWED) ? 1 : 0),
      AccountInfoInteger(ACCOUNT_LOGIN),
      (long)TimeCurrent() * 1000));
   gLastHeartbeat = TimeCurrent();
}

void CloseSocket(const string reason)
{
   if(gSocket != INVALID_HANDLE)
   {
      if(InpVerbose) Print("Mt5BridgeEA: closing socket (" + reason + ")");
      SocketClose(gSocket);
   }
   gSocket = INVALID_HANDLE;
   gHelloSent = false;
   gRxBuffer = "";
}

void EnsureConnected()
{
   if(gSocket != INVALID_HANDLE)
   {
      if(!SocketIsConnected(gSocket))
         CloseSocket("peer closed");
      else
         return;
   }
   if(gTokenRejected) return;

   datetime now = TimeCurrent();
   if(now < gLastConnectAttempt + gReconnectDelaySecs) return;
   gLastConnectAttempt = now;

   gSocket = SocketCreate();
   if(gSocket == INVALID_HANDLE)
   {
      PrintFormat("Mt5BridgeEA: SocketCreate failed (%d)", GetLastError());
      return;
   }
   SocketTimeouts(gSocket, 100, 1000);
   if(!SocketConnect(gSocket, InpBridgeHost, InpBridgePort, 1000))
   {
      if(InpVerbose)
         PrintFormat("Mt5BridgeEA: connect to %s:%d failed (%d)",
                     InpBridgeHost, InpBridgePort, GetLastError());
      SocketClose(gSocket);
      gSocket = INVALID_HANDLE;
      gReconnectDelaySecs = MathMin(gReconnectDelaySecs * 2, 30);
      return;
   }
   gReconnectDelaySecs = 2;
   PrintFormat("Mt5BridgeEA: connected to bridge at %s:%d (mode=%s login=%I64d)",
               InpBridgeHost, InpBridgePort, AccountMode(), AccountInfoInteger(ACCOUNT_LOGIN));
   gLastConnectAttempt = now + 1; // send HELLO on the next tick
}

//+------------------------------------------------------------------+
//| Terminal events                                                  |
//+------------------------------------------------------------------+
int OnInit()
{
   if(InpToken == "")
      Print("Mt5BridgeEA: WARNING — InpToken is empty, the bridge will reject the session " +
            "unless MT5_EA_TOKEN is likewise unset (read-only).");

   if(!IsDemoAccount())
      Print("Mt5BridgeEA: WARNING — this terminal is NOT a demo account. The EA and the bridge " +
            "will refuse every write method; attach it to a Deriv MT5 demo terminal.");

   EventSetMillisecondTimer(100);
   EnsureConnected();
   return INIT_SUCCEEDED;
}

void OnDeinit(const int reason)
{
   EventKillTimer();
   CloseSocket("deinit " + IntegerToString(reason));
}

void OnTimer()
{
   EnsureConnected();
   if(gSocket == INVALID_HANDLE) return;

   if(!gHelloSent) SendHello();

   DrainSocket();

   if(InpHeartbeatSecs > 0 && TimeCurrent() >= gLastHeartbeat + InpHeartbeatSecs)
      SendHeartbeat();
}

void OnTradeTransaction(const MqlTradeTransaction &transaction,
                        const MqlTradeRequest &request,
                        const MqlTradeResult &result)
{
   if(gSocket == INVALID_HANDLE || !gHelloSent) return;

   if(transaction.type == TRADE_TRANSACTION_DEAL_ADD)
   {
      ulong deal = transaction.deal;
      if(deal == 0) return;
      if(!HistoryDealSelect(deal)) return;
      long entry = HistoryDealGetInteger(deal, DEAL_ENTRY);
      string kind = "deal_" + DealEntryName(entry);
      SendLine(StringFormat(
         "EVT TRADE kind=%s ticket=%I64u position=%I64d symbol=%s side=%s volume=%s price=%s profit=%s magic=%I64d",
         kind, deal,
         HistoryDealGetInteger(deal, DEAL_POSITION_ID),
         Enc(HistoryDealGetString(deal, DEAL_SYMBOL)),
         (HistoryDealGetInteger(deal, DEAL_TYPE) == DEAL_TYPE_BUY) ? "buy" : "sell",
         DoubleToString(HistoryDealGetDouble(deal, DEAL_VOLUME), 8),
         DoubleToString(HistoryDealGetDouble(deal, DEAL_PRICE), 8),
         DoubleToString(HistoryDealGetDouble(deal, DEAL_PROFIT), 8),
         HistoryDealGetInteger(deal, DEAL_MAGIC)));
   }
}
//+------------------------------------------------------------------+

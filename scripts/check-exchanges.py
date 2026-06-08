#!/usr/bin/env python3
"""Manual health check for the XRC crypto exchange sources.

For each exchange and each canary asset, queries the exchange for ASSET/USDT
(mirroring the URL construction and price field used by src/xrc/src/exchanges.rs)
over a recent, already-closed minute and classifies the result:

  OK        - HTTP 200 and a usable price was extracted.
  NO DATA   - HTTP 200 but no candle (pair delisted / illiquid / dropped).
  HTTP <c>  - non-200 status (403 -> geo-block, 429 -> rate-limit, 5xx -> outage).
  API       - response shape/parse changed (extractor would break).
  NET       - network/transport error.

This is a single-vantage probe (your machine's IP). The IC makes the same
outcalls from many subnet node IPs and must reach consensus, so geo-block /
rate-limit issues that only hit a subset of those IPs may NOT be visible here.
NO DATA / API / pair-dropped issues, however, reproduce from anywhere.

Usage:
  scripts/check-exchanges.py                 # BTC, ETH, ICP vs USDT
  scripts/check-exchanges.py SOL XRD         # custom base assets
  scripts/check-exchanges.py --quote USDT
"""
import argparse
import json
import sys
import time
import urllib.request

DEFAULT_ASSETS = ["BTC", "ETH", "ICP"]


def http_get(url, timeout=15):
    req = urllib.request.Request(url, headers={"User-Agent": "xrc-exchange-healthcheck/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read().decode("utf-8", "replace"), None
    except urllib.error.HTTPError as e:
        body = ""
        try:
            body = e.read().decode("utf-8", "replace")
        except Exception:
            pass
        return e.code, body, None
    except Exception as e:  # noqa: BLE001 - report any transport failure
        return None, "", str(e)


def _num(x):
    return f"{float(x):g}"


# Each builder returns (url, extractor). The extractor receives the parsed JSON
# and returns the price string, raising on an unexpected shape, or returning
# None when there is no candle. Field indices mirror src/xrc/src/exchanges.rs.
def coinbase(base, quote, ts):
    url = (f"https://api.exchange.coinbase.com/products/{base}-{quote}/candles"
           f"?granularity=60&start={ts - 360}&end={ts - 60}")
    # array, newest-first, candle = [time, low, high, open, close, volume]; open = [3]
    return url, lambda j: (None if not j else _num(j[0][3]))


def kucoin(base, quote, ts):
    url = (f"https://api.kucoin.com/api/v1/market/candles?symbol={base}-{quote}"
           f"&type=1min&startAt={ts - 360}&endAt={ts - 60 + 1}")
    def ex(j):
        if j.get("code") != "200000":
            raise ValueError(f"code={j.get('code')}")
        data = j.get("data") or []
        return None if not data else _num(data[0][1])  # newest-first; open = [1]
    return url, ex


def okx(base, quote, ts):
    url = (f"https://www.okx.com/api/v5/market/history-candles?instId={base}-{quote}"
           f"&bar=1m&before={ts * 1000 - 60001}&after={ts * 1000 + 1}")
    def ex(j):
        if j.get("code") != "0":
            raise ValueError(f"code={j.get('code')} msg={j.get('msg')}")
        data = j.get("data") or []
        return None if not data else _num(data[0][1])  # newest-first; open = [1]
    return url, ex


def gateio(base, quote, ts):
    url = (f"https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair={base}_{quote}"
           f"&interval=1m&from={ts - 360}&to={ts - 60}")
    return url, lambda j: (None if not j else _num(j[0][3]))  # field [3]


def mexc(base, quote, ts):
    url = (f"https://api.mexc.com/api/v3/klines?symbol={base}{quote}"
           f"&interval=1m&startTime={ts - 60}&limit=1")
    return url, lambda j: (None if not j else _num(j[0][1]))  # open = [1]


def poloniex(base, quote, ts):
    url = (f"https://api.poloniex.com/markets/{base}_{quote}/candles"
           f"?interval=MINUTE_1&startTime={(ts - 360) * 1000}&endTime={(ts - 60) * 1000 + 1}")
    return url, lambda j: (None if not j else _num(j[-1][2]))  # oldest-first; newest open = [-1][2]


def cryptocom(base, quote, ts):
    url = (f"https://api.crypto.com/exchange/v1/public/get-candlestick"
           f"?instrument_name={base}_{quote}&timeframe=1m&start_ts={(ts - 60) * 1000}&count=1")
    def ex(j):
        data = (j.get("result") or {}).get("data") or []
        return None if not data else _num(data[0]["o"])
    return url, ex


def bitget(base, quote, ts):
    url = (f"https://api.bitget.com/api/v2/spot/market/history-candles?symbol={base}{quote}"
           f"&granularity=1min&endTime={(ts - 60) * 1000 + 60000}&limit=1")
    def ex(j):
        if j.get("code") != "00000":
            raise ValueError(f"code={j.get('code')} msg={j.get('msg')}")
        data = j.get("data") or []
        return None if not data else _num(data[0][1])  # open = [1]
    return url, ex


def digifinex(base, quote, ts):
    url = (f"https://openapi.digifinex.com/v3/kline?symbol={base}_{quote}"
           f"&period=1&start_time={ts - 360}&end_time={ts - 60}")
    def ex(j):
        if j.get("code") not in (0, "0"):
            raise ValueError(f"code={j.get('code')}")
        data = j.get("data") or []
        return None if not data else _num(data[0][5])  # field [5]
    return url, ex


EXCHANGES = [
    ("Coinbase", coinbase), ("KuCoin", kucoin), ("Okx", okx), ("GateIo", gateio),
    ("Mexc", mexc), ("Poloniex", poloniex), ("CryptoCom", cryptocom),
    ("Bitget", bitget), ("Digifinex", digifinex),
]


def coinbase_delisted(base, quote):
    """Best-effort: ask Coinbase whether the product is delisted, for richer detail."""
    status, body, err = http_get(f"https://api.exchange.coinbase.com/products/{base}-{quote}")
    if status == 200:
        try:
            return json.loads(body).get("status")
        except Exception:
            return None
    if status == 404:
        return "not found"
    return None


def classify(name, base, quote, ts):
    url, extractor = EXCHANGES_BY_NAME[name](base, quote, ts)
    status, body, neterr = http_get(url)
    if neterr is not None:
        return "NET", neterr
    if status != 200:
        hint = {403: " (geo-block?)", 429: " (rate-limited?)"}.get(status, "")
        return "HTTP", f"{status}{hint}"
    try:
        parsed = json.loads(body)
    except Exception as e:  # noqa: BLE001
        return "API", f"non-JSON body: {e}"
    try:
        price = extractor(parsed)
    except (KeyError, IndexError, TypeError, ValueError) as e:
        return "API", f"shape changed: {e}"
    if price is None:
        detail = "no candle returned (delisted/illiquid?)"
        if name == "Coinbase":
            st = coinbase_delisted(base, quote)
            if st:
                detail = f"no candle; product status={st}"
        return "NO DATA", detail
    return "OK", price


EXCHANGES_BY_NAME = dict(EXCHANGES)


def main():
    ap = argparse.ArgumentParser(description="XRC crypto exchange health check")
    ap.add_argument("assets", nargs="*", default=DEFAULT_ASSETS, help="base assets (default: BTC ETH ICP)")
    ap.add_argument("--quote", default="USDT", help="quote asset (default: USDT)")
    args = ap.parse_args()
    assets = [a.upper() for a in args.assets]
    quote = args.quote.upper()

    # A safely-closed minute (two minutes ago), floored to the minute.
    ts = ((int(time.time()) - 120) // 60) * 60

    print(f"XRC exchange health check — {quote} pairs, closed minute @ {ts} "
          f"({time.strftime('%Y-%m-%d %H:%M:%SZ', time.gmtime(ts))})")
    print("(single-vantage probe; IC-node geo/rate-limit may differ — see header)\n")

    header = f"{'exchange':<11}" + "".join(f"{a + '/' + quote:<18}" for a in assets)
    print(header)
    print("-" * len(header))

    issues = []  # (exchange, asset, status, detail)
    healthy_exchanges = []
    for name, _ in EXCHANGES:
        cells = []
        ok_count = 0
        for asset in assets:
            st, detail = classify(name, asset, quote, ts)
            if st == "OK":
                ok_count += 1
                cells.append(f"OK {detail}")
            else:
                cells.append(f"{st}")
                issues.append((name, asset, st, detail))
        if ok_count == len(assets):
            healthy_exchanges.append(name)
        print(f"{name:<11}" + "".join(f"{c:<18}" for c in cells))

    print("\n== Issues ==")
    if not issues:
        print("  none — all probed pairs returned a usable price.")
    else:
        for name, asset, st, detail in issues:
            print(f"  {name} {asset}/{quote}: {st} — {detail}")

    print(f"\nHealthy (all assets OK): {', '.join(healthy_exchanges) or 'none'}")
    print(f"Exchanges with >=1 issue: "
          f"{', '.join(sorted({n for n, *_ in issues})) or 'none'}")
    # Non-zero exit if anything is wrong, so it can gate a cron/CI check.
    return 1 if issues else 0


if __name__ == "__main__":
    sys.exit(main())

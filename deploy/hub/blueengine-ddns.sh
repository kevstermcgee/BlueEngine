#!/bin/sh
# Update the DuckDNS records for $DOMAIN (names without ".duckdns.org", comma separated) to this machine's public IP.
# Run by blueengine-ddns.service with DOMAIN and TOKEN from ~/.config/blueengine/duckdns.env. The token goes to curl on
# stdin (printf is a shell builtin), so it is never in an argument list and never printed.
set -eu
: "${DOMAIN:?DOMAIN is not set in duckdns.env}"
: "${TOKEN:?TOKEN is not set in duckdns.env}"
case "$DOMAIN" in
  *[!A-Za-z0-9,-]*) echo "DOMAIN may only hold letters, digits, hyphens and commas" >&2; exit 2 ;;
esac
case "$TOKEN" in
  *[!A-Za-z0-9-]*) echo "TOKEN has unexpected characters; copy it again from duckdns.org" >&2; exit 2 ;;
esac
# An empty ip= makes DuckDNS use the address the request came from.
reply=$(printf 'url = "https://www.duckdns.org/update?domains=%s&token=%s&ip="\n' "$DOMAIN" "$TOKEN" |
  curl -sS --max-time 20 -K -) || { echo "DuckDNS: could not reach the service" >&2; exit 1; }
if [ "$reply" = "OK" ]; then
  echo "DuckDNS: $DOMAIN updated"
else
  echo "DuckDNS: the update was refused (check DOMAIN and TOKEN in duckdns.env)" >&2
  exit 1
fi

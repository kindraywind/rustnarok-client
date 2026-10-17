"""Packets rathena's servers write by hand (`WFIFOW(fd,0) = 0x08e2;`, or `= cmd;` after `int32 cmd = 0xac2;`), which
neither packet_db nor the packet structs list: prints "0x<id> <length>" for each one in the given preprocessed sources
(so only what this PACKETVER sends).
The length is the literal in the WFIFOSET or clif_send that sends it, or -1 when that's computed and offset 2 holds a
16-bit field; packets sent with their packet_db length are packet_db's, and anything else is reported on stderr. IDs
from 0x2700 up are rathena's server-to-server packets.

Usage: python3 -I raw_headers.py <file.ii>...
"""
import re
import sys

# A 16-bit write at an offset of the send buffer (`WFIFOW(fd,n)` or `WBUFW(buf,n)`), as the preprocessor expands it.
WRITE = r"\(\*\(uint16\*\)\((?:session\[[^\]]+\]->wdata \+ session\[[^\]]+\]->wdata_size \+ \({0}\)|\(\(uint8\*\)\(\([\w.>-]+\)\)\) \+ \(\({0}\)\))\)\)\s*="
HEADER = re.compile(WRITE.format(0) + r"\s*(0x[0-9a-fA-F]+|\d+|[A-Za-z_]\w*)\s*;")
FIELD_AT_2 = re.compile(WRITE.format(2))
SEND = re.compile(r"\b(?:WFIFOSET|clif_send)\(\s*[^,]+,\s*([^,)]+)")

lengths = {}
for path in sys.argv[1:]:
    text = open(path, encoding='latin-1').read()
    for header in HEADER.finditer(text):
        value = header.group(1)
        if not value[0].isdigit():
            # A variable: the last number given it earlier in the same function (one ends at a `}` in column 0).
            body = text[text.rfind('\n}', 0, header.start()) + 1:header.start()]
            given = re.findall(r'\b' + value + r'\s*=\s*(0x[0-9a-fA-F]+|\d+)\s*;', body)
            if not given:
                continue
            value = given[-1]
        packet = int(value, 0)
        if packet >= 0x2700:
            continue
        # The next send is this packet's (headers chosen by an if/else share the one after them).
        send = SEND.search(text, header.end())
        size = send.group(1).strip() if send else ''
        if 'packet_db[' in size:
            continue
        if size.isdigit():
            lengths.setdefault(packet, int(size))
        elif send and FIELD_AT_2.search(text, header.end(), send.start()):
            lengths.setdefault(packet, -1)
        else:
            print(f'0x{packet:04X}: no length found ({size or "not sent"})', file=sys.stderr)
for packet, length in sorted(lengths.items()):
    print(f'0x{packet:04X} {length}')

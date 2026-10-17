"""Pairs rathena's old-style `enum packet_headers` entries (`nameType` or plain `name`) with the struct clif.cpp
actually sends them as.

Prints "<name> <struct>" per header used in clif.cpp; entries clif.cpp never assigns are unused leftovers.
Usage: python3 -I old_headers.py <rathena>/src/map
"""
import re
import sys

root = sys.argv[1]
source = open(f'{root}/clif.cpp', encoding='latin-1').read()
headers = open(f'{root}/packets_struct.hpp', encoding='latin-1').read()
start = headers.index('enum packet_headers')
enum = headers[start:headers.index('};', start)]
for name in sorted(set(re.findall(r'\b([A-Za-z0-9_]+)\s*=\s*0x[0-9a-fA-F]+', enum))):
    assignment = re.search(r'(\w+)\s*(?:\.|->)\s*[Pp]acket[Tt]ype\s*=\s*' + name + r'\b', source)
    if not assignment:
        continue
    declarations = re.findall(r'(?:struct\s+)?\b((?:PACKET|packet)_[A-Za-z0-9_]+)\s*[*&]?\s*' + re.escape(assignment.group(1)) + r'\b',
                              source[:assignment.start()])
    if declarations:
        print(name, declarations[-1])

#!/bin/bash
# Generates the packet length table rathena uses for PACKETVER 20220406 (login, char and map packets).
# Usage: run.sh <rathena dir> <output file>   (writes scratch files next to this script)
set -euo pipefail
G="$(cd "$(dirname "$0")" && pwd)"
cd "$1/src/map"
FLAGS=(-std=c++17 -ferror-limit=0 -Wno-everything -I"$G" -I. -I../common/ -I../ -I../../3rdparty/libconfig -I/opt/homebrew/opt/pcre/include
       -I../../3rdparty/rapidyaml/src -I../../3rdparty/rapidyaml/ext/c4core/src -I/opt/homebrew/opt/mysql-client/include/mysql
       -DPACKETVER=20220406 -DHAS_TLS -DHAVE_GETADDRINFO -DHAVE_SETRLIMIT -DHAVE_STRNLEN -DHAVE_MONOTONIC_CLOCK)

# 1. packet_db: clif_packetdb.hpp + clif_shuffle.hpp, with the registration macro swapped for a recorder.
for f in clif_packetdb.hpp clif_shuffle.hpp; do
  sed -E -e 's/^[[:space:]]*#define packet\(cmd,length\).*/#define packet(cmd,length) add_packet(cmd,length)/' \
         -e 's/^[[:space:]]*#define parseable_packet\(cmd,length,func,...\).*/#define parseable_packet(cmd,length,func,...) add_packet(cmd,length)/' \
         -e 's/^(#(ifndef|define|endif).*CLIF_(PACKETDB|SHUFFLE)_HPP.*)$/\/\/ \1/' "$f" > "$G/$f"
done
g++ "${FLAGS[@]}" "$G/gen.cpp" -o "$G/gen"
"$G/gen" > "$G/db.txt"

# 2. Struct-defined packets: every HEADER_x active at this PACKETVER, sized by sizeof(PACKET_x); structs with a
#    packetLength (PacketLength, packetLen, packetSize, length) member are variable length. Map and login/char headers clash, so each gets its own pass.
struct_pass() { # <tag> <include line>
  local tag="$1" include="$2"
  echo "$include" > "$G/pp_$tag.cpp"
  g++ "${FLAGS[@]}" -E "$G/pp_$tag.cpp" -o "$G/pp_$tag.ii"
  grep -oE 'const int16 HEADER_[A-Za-z0-9_]+ = (0x[0-9a-fA-F]+|[0-9]+)' "$G/pp_$tag.ii" | awk '{print $3}' | sed 's/^HEADER_//' | sort -u > "$G/names_$tag.txt"
  emit() {
    {
      echo '#include <cstdio>'
      echo '#include <type_traits>'
      echo "$include"
      echo 'template<typename T, typename = void> struct lower : std::false_type {};'
      echo 'template<typename T> struct lower<T, std::void_t<decltype(&T::packetLength)>> : std::true_type {};'
      echo 'template<typename T, typename = void> struct upper : std::false_type {};'
      echo 'template<typename T> struct upper<T, std::void_t<decltype(&T::PacketLength)>> : std::true_type {};'
      echo 'template<typename T, typename = void> struct shorter : std::false_type {};'
      echo 'template<typename T> struct shorter<T, std::void_t<decltype(&T::packetLen)>> : std::true_type {};'
      echo 'template<typename T, typename = void> struct sized : std::false_type {};'
      echo 'template<typename T> struct sized<T, std::void_t<decltype(&T::packetSize)>> : std::true_type {};'
      echo 'template<typename T, typename = void> struct plain : std::false_type {};'
      echo 'template<typename T> struct plain<T, std::void_t<decltype(&T::length)>> : std::true_type {};'
      echo '#define ENTRY(name) std::printf("0x%04X %d\n", (int)(uint16)HEADER_##name, lower<PACKET_##name>::value || upper<PACKET_##name>::value || shorter<PACKET_##name>::value || sized<PACKET_##name>::value || plain<PACKET_##name>::value ? -1 : (int)sizeof(PACKET_##name));'
      echo 'int main() {'
      sed 's/.*/ENTRY(&)/' "$1"
      echo '}'
    } > "$G/gen2_$tag.cpp"
  }
  emit "$G/names_$tag.txt"
  # Drop headers whose struct isn't named PACKET_<header> (packet_db covers those).
  g++ "${FLAGS[@]}" -fsyntax-only "$G/gen2_$tag.cpp" 2>&1 | grep -oE "PACKET_[A-Za-z0-9_]+" | sed 's/^PACKET_//' | sort -u > "$G/missing_$tag.txt" || true
  grep -vxF -f "$G/missing_$tag.txt" "$G/names_$tag.txt" > "$G/kept_$tag.txt" || cp "$G/names_$tag.txt" "$G/kept_$tag.txt"
  emit "$G/kept_$tag.txt"
  g++ "${FLAGS[@]}" "$G/gen2_$tag.cpp" -o "$G/gen2_$tag"
  "$G/gen2_$tag" > "$G/structs_$tag.txt"
}
struct_pass map '#include "packets.hpp"'
struct_pass common '#include <common/packets.hpp>'

# 3. Old-style map headers (`enum packet_headers { nameType = id }`), sized by the struct clif.cpp sends them as.
# Only entries active at this PACKETVER (the enum is conditional too); a literal ID clif.cpp assigns passes as is.
awk '/enum packet_headers \{/{inside=1} inside{print} inside && /\};/{exit}' "$G/pp_map.ii" \
  | grep -oE '[A-Za-z0-9_]+ = 0x[0-9a-fA-F]+' | awk '{print $1}' | sort -u > "$G/names_old.txt"
python3 -I "$G/old_headers.py" . | awk 'NR == FNR { active[$1] = 1; next } $1 in active || $1 ~ /^0x/' "$G/names_old.txt" - > "$G/old_pairs.txt"
{
  echo '#include <cstdio>'
  echo '#include <type_traits>'
  echo '#include "packets.hpp"'
  echo 'template<typename T, typename = void> struct lower : std::false_type {};'
  echo 'template<typename T> struct lower<T, std::void_t<decltype(&T::packetLength)>> : std::true_type {};'
  echo 'template<typename T, typename = void> struct upper : std::false_type {};'
  echo 'template<typename T> struct upper<T, std::void_t<decltype(&T::PacketLength)>> : std::true_type {};'
  echo 'template<typename T, typename = void> struct shorter : std::false_type {};'
  echo 'template<typename T> struct shorter<T, std::void_t<decltype(&T::packetLen)>> : std::true_type {};'
  echo 'template<typename T, typename = void> struct sized : std::false_type {};'
  echo 'template<typename T> struct sized<T, std::void_t<decltype(&T::packetSize)>> : std::true_type {};'
  echo 'template<typename T, typename = void> struct plain : std::false_type {};'
  echo 'template<typename T> struct plain<T, std::void_t<decltype(&T::length)>> : std::true_type {};'
  echo '#define ENTRY(name, type) std::printf("0x%04X %d\n", (int)(uint16)name, lower<type>::value || upper<type>::value || shorter<type>::value || sized<type>::value || plain<type>::value ? -1 : (int)sizeof(type));'
  echo 'int main() {'
  awk '{ print "ENTRY(" $1 ", " $2 ")" }' "$G/old_pairs.txt"
  echo '}'
} > "$G/gen3.cpp"
g++ "${FLAGS[@]}" "$G/gen3.cpp" -o "$G/gen3"
"$G/gen3" > "$G/structs_old.txt"

# 4. Packets written by hand (`WFIFOW(fd,0) = 0x08e2`, or a variable set to the ID) in the sources that talk to the
#    client, as this PACKETVER compiles them, sized by the WFIFOSET or clif_send that sends them.
for source in map/clif.cpp char/char_clif.cpp login/loginclif.cpp; do
  g++ "${FLAGS[@]}" -E -P "../$source" -o "$G/raw_$(basename "$source" .cpp).ii"
done
python3 -I "$G/raw_headers.py" "$G"/raw_*.ii > "$G/raw.txt"

# Merge: a packet the server sends as a struct is what the struct says (its length field, whatever it's called,
# makes it variable); packet_db keeps legacy lengths and placeholders for those (0x01D7 is 11 there but 15 on the
# wire, 0x02F7 a 2008 placeholder of -1 but 47 now). packet_db fills in the rest, and hand-written packets what's left.
{
  echo '# Packet lengths (-1 = variable, length at offset 2) rathena uses for PACKETVER 20220406.'
  echo '# Generated by tools/packet-lengths/run.sh from rathena headers.'
  awk 'FILENAME == ARGV[1] { raw[$1] = $2; next } FILENAME == ARGV[2] { db[$1] = $2; next } { structs[$1] = $2 }
       END {
         for (id in raw) if (!(id in db) && !(id in structs)) print id, raw[id]
         for (id in db) if (!(id in structs)) print id, db[id]
         for (id in structs) print id, structs[id]
       }' "$G/raw.txt" "$G/db.txt" "$G/structs_common.txt" "$G/structs_map.txt" "$G/structs_old.txt" | sort
} > "$2"
echo "packet_db $(wc -l < "$G/db.txt"), map structs $(wc -l < "$G/structs_map.txt"), login/char structs $(wc -l < "$G/structs_common.txt"), old-style $(wc -l < "$G/structs_old.txt"), hand-written $(wc -l < "$G/raw.txt"); total $(grep -c '^0x' "$2")"

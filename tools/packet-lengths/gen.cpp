// Prints rathena's packet length table (id length; -1 = variable) for the PACKETVER it's compiled with.
#include <cstdio>
#include <map>
#include "packets.hpp"

static std::map<int, int> lengths;
static void add_packet(int command, int length) { lengths[command] = length; }

int main() {
#include "clif_packetdb.hpp"
#include "clif_shuffle.hpp"
	for (auto& [command, length] : lengths)
		std::printf("0x%04X %d\n", command, length);
}

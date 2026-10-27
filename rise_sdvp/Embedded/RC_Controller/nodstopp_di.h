/*
	Nödstopp på MP101:s DI1 (bara med byggflaggan NODSTOPP_DI1). Se nodstopp_di.c.
 */

#ifndef NODSTOPP_DI_H_
#define NODSTOPP_DI_H_

#include <stdbool.h>
#include <stdint.h>

void nodstopp_di_init(void);
// Sant = kommandot ska inte utföras (det kan få roboten att röra sig medan nödstoppet spärrar).
// data/len är kommandots data efter paket-ID, som i commands_process_packet.
bool nodstopp_di_sparrar(uint8_t packet_id, const unsigned char *data, unsigned int len);

#endif /* NODSTOPP_DI_H_ */

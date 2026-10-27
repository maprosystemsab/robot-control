/*
	Lägesknapp på MP101:s DI2 och lampa på DI3 (bara med byggflaggan LAGESKNAPP_DI2).
	Se lagesknapp_di.c.
 */

#ifndef LAGESKNAPP_DI_H_
#define LAGESKNAPP_DI_H_

#include <stdint.h>

#define LAGESKNAPP_CMD_LAGE		144	// kort -> klienter: [id][144][läge] 0 släppt, 1 intryckt, 2 hållen 3 s
#define LAGESKNAPP_CMD_LAMPA	145	// klient -> kort: [id][145][läge] 0 av, 1 på, 2 blink

void lagesknapp_di_init(void);
void lagesknapp_di_lampa(uint8_t lage);

#endif /* LAGESKNAPP_DI_H_ */

/*
	Styrning med vinkelgivare (RobAnt): spakens utslag = hjulens vinkel.
	Se styr_position.c.
 */

#ifndef STYR_POSITION_H_
#define STYR_POSITION_H_

#include <stdbool.h>

// Activity för styrningen i aktuatorlistan (RControlStation "Steering").
#define STYRPOS_ACTIVITY	11

void styrpos_init(void);
void styrpos_update(void);
bool styrpos_available(void);
// Tar hand om ett styrkommando (spakens utslag). Falskt = kör det gamla sättet.
bool styrpos_command(int vesc_id, float magnitude);

#endif /* STYR_POSITION_H_ */

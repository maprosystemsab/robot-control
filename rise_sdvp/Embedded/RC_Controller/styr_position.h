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
// Vinkelgivaren är OK och kalibrerad (oavsett om vinkelstyrningen är på).
bool styrpos_vinkel_giltig(void);
// Autocentrering för knappen i RCS/Robotstyrning (CMD_GET_VINKEL, CMD_SET_AUTOCENTRERING).
#define STYRPOS_STATUS_INGEN_GIVARE	0	// ingen kalibrerad givare: knappen grå och låst
#define STYRPOS_STATUS_PA			1	// grön
#define STYRPOS_STATUS_AV			2	// grå, avslagen av föraren (sparat)
#define STYRPOS_STATUS_FEL			3	// grå, avslagen av ett givarfel/styrfel
int styrpos_status(void);
bool styrpos_set_autocentrering(bool on);
// Tar hand om ett styrkommando (spakens utslag). Falskt = kör det gamla sättet.
bool styrpos_command(int vesc_id, float magnitude);
// Samma, men målet direkt i grader (+ = höger), t.ex. från autopiloten.
bool styrpos_target_deg(int vesc_id, float deg);

#endif /* STYR_POSITION_H_ */

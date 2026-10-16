/*
	Styrning med vinkelgivare (RobAnt, 2026-10-15)

	Spakens utslag blir hjulens vinkel: lite spak = lite sväng, släppt spak =
	hjulen går tillbaka till rakt fram av sig själva. Utan detta styr spaken
	styrmotorns duty direkt, och hjulen blir stående där de är när spaken släpps.

	Kortet driver styr-VESC:en (activity 11) mot målvinkeln var 10:e ms: full
	fart (max) tills det är m_zone_deg kvar, sedan sakta in linjärt från
	m_slow_duty ner till m_min_duty vid dödbandet (önskemål 2026-10-15: snabbt
	tillbaka, mjukt sista 10–15 %). Faller tillbaka till det gamla läget (spaken = duty) om
	vinkelgivaren inte är OK eller inte är kalibrerad (vinkel_noll/höger/vänster),
	eller om läget slås av med terminalkommandot `styrlage duty`.

	Säkerhet:
	- Efter sista styrkommandot körs bara tills målvinkeln är nådd, högst
	  STYRPOS_HOLD_MS. Sedan duty 0.
	- Givarfel under körning: duty 0, och nästa kommando går i det gamla läget.
	- Går vinkeln åt fel håll (fel tecken) i STYRPOS_WRONG_MS: duty 0 och
	  läget slås av tills `styrlage vinkel` körs igen.
	- Står vinkeln still fast motorn drivs i STYRPOS_STALL_MS (givaren har
	  fastnat, t.ex. FMV 2026-10-13, eller styrningen kärvar): samma sak.

	Autocentrering (30.18, 2026-10-16): knappen "Autocentrering" i RCS och
	Robotstyrning slår läget på/av (CMD_SET_AUTOCENTRERING / terminal
	`autocentrering pa|av`), sparas i styrkortet. Standard på. Felen ovan, och
	givare som slutar svara eller visar orimlig vinkel (STYRPOS_FEL_MS), slår av
	läget tills föraren slår på det igen; status 3 i CMD_GET_VINKEL och orsaken
	i terminalen. `styrlage duty` (kalibreringen) slår bara av tillfälligt.
 */

#include "styr_position.h"
#include "conf_general.h"
#include "terminal.h"
#include "commands.h"
#include "comm_can.h"
#include "bldc_interface.h"
#include "adconv.h"
#include "utils.h"
#include "ch.h"
#include <math.h>
#include <string.h>
#include <stdlib.h>
#include <stdio.h>

#ifdef ANGLE_SENSOR_PA3

extern float frontangle;

// Spakens fulla utslag så som RControlStation och robotd skickar det (styrning alltid 0,8).
#define STYRPOS_FULL_STICK		0.8
// Målvinkel vid fullt utslag: lite innanför kalibrerat ändläge (±ANGLE_SENSOR_MAX_DEG),
// så att motorn inte trycker mot ändstoppet.
#define STYRPOS_MAX_DEG			(ANGLE_SENSOR_MAX_DEG * 0.93)
#define STYRPOS_HOLD_MS			3000
#define STYRPOS_WRONG_MS		600
#define STYRPOS_STALL_MS		1500
#define STYRPOS_STALL_DEG		0.5
#define STYRPOS_STALL_MIN_ERR	5.0

static float m_zone_deg = 4.5;		// sakta in de sista graderna (15 % av 30°)
static float m_slow_duty = 0.2;		// duty där inbromsningen börjar
static float m_max_duty = 0.8;		// samma kraft som manuell styrning (spaken skickar alltid 0,8)
static float m_min_duty = 0.08;		// minsta duty som rubbar styrningen
// Stannar inom ±4 % av fullt utslag och börjar inte rätta igen förrän felet är
// över 10 % (eller spaken flyttas). Utan glappet småstyrde den hela tiden kring
// målet (mk1 2026-10-15).
static float m_deadband_deg = ANGLE_SENSOR_MAX_DEG * 0.04;
static float m_restart_deg = ANGLE_SENSOR_MAX_DEG * 0.10;
static volatile bool m_settled = false;	// står på målet; vänta på ny spak eller stort fel
// Står hjulen still under inbromsningen (mer friktion när drivhjulen snurrar,
// mk1 2026-10-15: fastnade på 2,5° med duty 0,12) ökas kraften stegvis tills de
// rör sig igen. Nollas när målet nås eller ett nytt mål kommer.
#define STYRPOS_BOOST_STEG		0.04
#define STYRPOS_BOOST_MS		150
#define STYRPOS_BOOST_RORELSE	0.2	// grader per mätperiod som räknas som rörelse
static float m_boost = 0.0;
static float m_boost_angle = 0.0;
static systime_t m_boost_t = 0;
static float m_sign = 1.0;			// +1: positiv duty ökar vinkeln (åt höger)
static bool m_enabled = true;		// styrlage vinkel/duty (tillfälligt, t.ex. under kalibreringen)
static bool m_user_on = true;		// knappen Autocentrering, sparad i styrkortet
static bool m_fault = false;		// avslagen av ett fel tills föraren slår på igen
static bool m_sensor_fault = false;	// felet gäller givaren: vinkeln används inte alls (ej differential)
static char m_fault_why[96] = "";
// Givaren svarar inte / visar orimlig vinkel så här länge -> autocentreringen av.
#define STYRPOS_FEL_MS			300
#define STYRPOS_ORIMLIG_DEG		(ANGLE_SENSOR_MAX_DEG * 1.3)
static systime_t m_ok_since = 0;

static volatile int m_vesc_id = -1;
static volatile float m_target_deg = 0.0;
static volatile systime_t m_last_cmd = 0;
static bool m_driving = false;
static float m_wrong_start_angle = 0.0;
static float m_wrong_dir = 0.0;
static systime_t m_wrong_since = 0;
static bool m_wrong_timing = false;
static systime_t m_stall_since = 0;
static float m_stall_angle = 0.0;
static bool m_stall_timing = false;

static void set_duty(int id, float duty) {
	comm_can_lock_vesc();
	comm_can_set_vesc_id(id);
	bldc_interface_set_duty_cycle(duty);
	comm_can_unlock_vesc();
}

bool styrpos_vinkel_giltig(void) {
	return !m_sensor_fault && adconv_angle_sensor_ok() && adconv_angle_calibrated();
}

bool styrpos_available(void) {
	return m_user_on && !m_fault && m_enabled && adconv_angle_sensor_ok() && adconv_angle_calibrated();
}

int styrpos_status(void) {
	if (!adconv_angle_calibrated()) {
		return STYRPOS_STATUS_INGEN_GIVARE;
	}
	if (!m_user_on) {
		return STYRPOS_STATUS_AV;
	}
	return m_fault ? STYRPOS_STATUS_FEL : STYRPOS_STATUS_PA;
}

static void print_status(void) {
	switch (styrpos_status()) {
	case STYRPOS_STATUS_INGEN_GIVARE:
		commands_printf("Autocentrering: ingen kalibrerad vinkelgivare, spaken styr motorn direkt.\n");
		break;
	case STYRPOS_STATUS_AV:
		commands_printf("Autocentrering: AV (spaken styr motorn direkt). Sparat i styrkortet.\n");
		break;
	case STYRPOS_STATUS_FEL:
		commands_printf("Autocentrering: AV p.g.a. fel: %s. Tryck Autocentrering för att slå på igen.\n", m_fault_why);
		break;
	default:
		commands_printf("Autocentrering: PÅ (spaken = hjulens vinkel, centrerar själv). Sparat i styrkortet.\n");
		break;
	}
}

static void stop(void);

bool styrpos_set_autocentrering(bool on) {
	if (!on) {
		stop();
	}
	m_user_on = on;
	m_fault = false;
	m_sensor_fault = false;
	m_enabled = true;
	m_ok_since = chVTGetSystemTimeX();
	bool ok = conf_general_store_autocentrering(on);
	if (!ok) {
		commands_printf("Autocentrering: kunde inte spara i styrkortet (gäller till omstart).\n");
	}
	print_status();
	return ok;
}

bool styrpos_target_deg(int vesc_id, float deg) {
	if (!styrpos_available()) {
		return false;
	}
	utils_truncate_number(&deg, -STYRPOS_MAX_DEG, STYRPOS_MAX_DEG);
	if (fabsf(deg - m_target_deg) > 0.5) {
		m_settled = false;	// nytt mål: kör dit
		m_boost = 0.0;
	}
	m_target_deg = deg;
	m_vesc_id = vesc_id;
	m_last_cmd = chVTGetSystemTimeX();
	return true;
}

bool styrpos_command(int vesc_id, float magnitude) {
	float s = magnitude / STYRPOS_FULL_STICK;
	utils_truncate_number(&s, -1.0, 1.0);
	return styrpos_target_deg(vesc_id, s * STYRPOS_MAX_DEG);
}

static void stop(void) {
	if (m_driving && m_vesc_id >= 0) {
		set_duty(m_vesc_id, 0.0);
	}
	m_driving = false;
	m_wrong_timing = false;
	m_stall_timing = false;
	m_boost = 0.0;
}

static void give_up(const char *why, bool sensor) {
	if (m_vesc_id >= 0 && m_driving) {
		set_duty(m_vesc_id, 0.0);
	}
	m_driving = false;
	m_wrong_timing = false;
	m_stall_timing = false;
	m_fault = true;
	m_sensor_fault = sensor;
	strncpy(m_fault_why, why, sizeof(m_fault_why) - 1);
	m_fault_why[sizeof(m_fault_why) - 1] = '\0';
	commands_printf("Styrning: %s. Autocentreringen avslagen, spaken styr motorn direkt "
			"(knappen Autocentrering slår på den igen).\n", why);
}

// Givaren slutar svara eller visar en vinkel långt utanför kalibreringen:
// slå av autocentreringen (FMV 2026-10-13: givaren stod fast på 1064 mV).
static void check_sensor(void) {
	if (!m_user_on || m_fault || !adconv_angle_calibrated()) {
		m_ok_since = chVTGetSystemTimeX();
		return;
	}
	bool ok = adconv_angle_sensor_ok() && fabsf(frontangle) < STYRPOS_ORIMLIG_DEG;
	if (ok) {
		m_ok_since = chVTGetSystemTimeX();
	} else if (chVTTimeElapsedSinceX(m_ok_since) > MS2ST(STYRPOS_FEL_MS)) {
		if (!adconv_angle_sensor_ok()) {
			give_up("vinkelgivaren svarar inte (kabel/magnet?)", true);
		} else {
			static char why[64];
			snprintf(why, sizeof(why), "vinkelgivaren visar %.0f grader, utanför kalibreringen", (double)frontangle);
			give_up(why, true);
		}
	}
}

void styrpos_update(void) {
	check_sensor();
	if (m_vesc_id < 0) {
		return;
	}
	if (!styrpos_available()) {
		stop();
		return;
	}

	float err = m_target_deg - frontangle;
	if (m_settled && fabsf(err) < m_restart_deg) {
		// Står på målet: rätta inte småfel, bara stora (se m_restart_deg).
		if (m_driving) {
			set_duty(m_vesc_id, 0.0);
			m_driving = false;
		}
		m_wrong_timing = false;
		m_stall_timing = false;
		return;
	}
	m_settled = false;
	bool reached = fabsf(err) < m_deadband_deg;
	if (reached) {
		m_settled = true;
		m_boost = 0.0;
	}
	bool stale = chVTTimeElapsedSinceX(m_last_cmd) > MS2ST(STYRPOS_HOLD_MS);
	bool cmd_recent = chVTTimeElapsedSinceX(m_last_cmd) < MS2ST(500);

	// Inga nya kommandon: kör bara klart till målet, sedan stilla.
	if (reached || stale) {
		if (reached && cmd_recent) {
			// Står på målet medan föraren styr: håll stilla utan att släppa läget.
			if (m_driving) {
				set_duty(m_vesc_id, 0.0);
				m_driving = false;
			}
			m_wrong_timing = false;
			m_stall_timing = false;
			return;
		}
		stop();
		return;
	}

	float a = fabsf(err);
	float mag;
	if (a >= m_zone_deg) {
		mag = m_max_duty;
		m_boost = 0.0;
		m_boost_angle = frontangle;
		m_boost_t = chVTGetSystemTimeX();
	} else {
		// Står still i inbromsningen: öka kraften stegvis.
		if (chVTTimeElapsedSinceX(m_boost_t) > MS2ST(STYRPOS_BOOST_MS)) {
			if (fabsf(frontangle - m_boost_angle) < STYRPOS_BOOST_RORELSE) {
				m_boost += STYRPOS_BOOST_STEG;
			}
			m_boost_angle = frontangle;
			m_boost_t = chVTGetSystemTimeX();
		}
		float span = m_zone_deg - m_deadband_deg;
		float t = span > 0.01 ? (a - m_deadband_deg) / span : 1.0;
		utils_truncate_number(&t, 0.0, 1.0);
		mag = m_min_duty + (m_slow_duty - m_min_duty) * t + m_boost;
		if (mag > m_max_duty) {
			mag = m_max_duty;
			m_boost = m_max_duty;	// håll boosten begränsad
		}
	}
	if (mag < m_min_duty) {
		mag = m_min_duty;
	}
	float duty = SIGN(err) * mag;
	duty *= m_sign;

	// Fel håll: hjulen rör sig bort från målet trots att motorn drivs mot det
	// -> stoppa och slå av läget. Mäts på vinkelns rörelse (inte på felet, som
	// växer av sig självt när spaken dras fortare än hjulen hinner med).
	float dir = SIGN(err);
	if (!m_wrong_timing || dir != m_wrong_dir) {
		m_wrong_timing = true;
		m_wrong_dir = dir;
		m_wrong_since = chVTGetSystemTimeX();
		m_wrong_start_angle = frontangle;
	} else if (chVTTimeElapsedSinceX(m_wrong_since) > MS2ST(STYRPOS_WRONG_MS)) {
		float moved = (frontangle - m_wrong_start_angle) * dir;	// + = mot målet
		if (moved < -3.0) {
			give_up("hjulen gick åt fel håll (byt tecken med styrlage tecken)", false);
			return;
		}
		// Nytt mätfönster
		m_wrong_since = chVTGetSystemTimeX();
		m_wrong_start_angle = frontangle;
	}

	// Står still: motorn drivs men vinkeln ändras inte (givaren har fastnat
	// eller styrningen kärvar) -> stoppa hellre än att driva vidare i blindo.
	// Bara vid stort fel: nära målet kan minsta duty vara för svag för att rubba
	// styrningen, och då är det inget fel.
	if (fabsf(err) < STYRPOS_STALL_MIN_ERR) {
		m_stall_timing = false;
	} else if (!m_stall_timing) {
		m_stall_timing = true;
		m_stall_since = chVTGetSystemTimeX();
		m_stall_angle = frontangle;
	} else if (fabsf(frontangle - m_stall_angle) > STYRPOS_STALL_DEG) {
		m_stall_since = chVTGetSystemTimeX();
		m_stall_angle = frontangle;
	} else if (chVTTimeElapsedSinceX(m_stall_since) > MS2ST(STYRPOS_STALL_MS)) {
		give_up("vinkeln ändras inte fast motorn drivs (givaren fast eller styrningen kärvar)", true);
		return;
	}

	set_duty(m_vesc_id, duty);
	m_driving = true;
}

static void terminal_cmd_styrlage(int argc, const char **argv) {
	if (argc >= 2) {
		if (strcmp(argv[1], "duty") == 0) {
			stop();
			m_enabled = false;
		} else if (strcmp(argv[1], "vinkel") == 0) {
			m_enabled = true;
			m_fault = false;
			m_sensor_fault = false;
		} else if (strcmp(argv[1], "tecken") == 0) {
			stop();
			m_sign = -m_sign;
		} else if (argc >= 3 && strcmp(argv[1], "zon") == 0) {
			m_zone_deg = (float)atof(argv[2]);
		} else if (argc >= 3 && strcmp(argv[1], "sakta") == 0) {
			m_slow_duty = (float)atof(argv[2]);
		} else if (argc >= 3 && strcmp(argv[1], "max") == 0) {
			m_max_duty = (float)atof(argv[2]);
		} else if (argc >= 3 && strcmp(argv[1], "min") == 0) {
			m_min_duty = (float)atof(argv[2]);
		} else if (argc >= 3 && strcmp(argv[1], "dodband") == 0) {
			m_deadband_deg = (float)atof(argv[2]);
		} else if (argc >= 3 && strcmp(argv[1], "aterstart") == 0) {
			m_restart_deg = (float)atof(argv[2]);
		} else {
			commands_printf("Okänt: styrlage [vinkel|duty|tecken|max X|zon X|sakta X|min X|dodband X|aterstart X]\n");
			return;
		}
	}
	commands_printf("Styrläge: %s%s. max %.2f, saktar in sista %.1f° från %.2f till %.2f, stannar inom ±%.1f°, rättar igen över %.1f°, tecken %+.0f. "
			"Mål %.1f°, nu %.1f°. Nollställs vid omstart.\n",
			m_enabled ? "vinkel (spaken = hjulens vinkel)" : "duty (spaken = motorns fart)",
			(m_enabled && !styrpos_available()) ? " – autocentrering av eller givaren ej OK/kalibrerad, kör duty" : "",
			(double)m_max_duty, (double)m_zone_deg, (double)m_slow_duty, (double)m_min_duty, (double)m_deadband_deg, (double)m_restart_deg, (double)m_sign,
			(double)m_target_deg, (double)frontangle);
	print_status();
}

static void terminal_cmd_autocentrering(int argc, const char **argv) {
	if (argc >= 2) {
		if (strcmp(argv[1], "pa") == 0 || strcmp(argv[1], "på") == 0 || strcmp(argv[1], "1") == 0) {
			styrpos_set_autocentrering(true);
			return;
		} else if (strcmp(argv[1], "av") == 0 || strcmp(argv[1], "0") == 0) {
			styrpos_set_autocentrering(false);
			return;
		}
	}
	print_status();
}

void styrpos_init(void) {
	m_user_on = conf_general_autocentrering();
	m_ok_since = chVTGetSystemTimeX();
	terminal_register_command_callback(
			"autocentrering",
			"Autocentrering (spaken = hjulens vinkel) på/av, sparas i styrkortet. Utan argument: visa läget.",
			"[pa|av]",
			terminal_cmd_autocentrering);
	terminal_register_command_callback(
			"styrlage",
			"Styrning: vinkel (spaken = hjulens vinkel, centrerar själv) eller duty (som förut). "
			"Även tecken, max X (full fart), zon X (grader där den saktar in), sakta X, min X, dodband X (grader), aterstart X (grader).",
			"[vinkel|duty|tecken|max X|zon X|sakta X|min X|dodband X|aterstart X]",
			terminal_cmd_styrlage);
}

#else

void styrpos_init(void) {}
void styrpos_update(void) {}
bool styrpos_available(void) { return false; }
bool styrpos_vinkel_giltig(void) { return false; }
int styrpos_status(void) { return STYRPOS_STATUS_INGEN_GIVARE; }
bool styrpos_set_autocentrering(bool on) { (void)on; return false; }
bool styrpos_command(int vesc_id, float magnitude) { (void)vesc_id; (void)magnitude; return false; }
bool styrpos_target_deg(int vesc_id, float deg) { (void)vesc_id; (void)deg; return false; }

#endif

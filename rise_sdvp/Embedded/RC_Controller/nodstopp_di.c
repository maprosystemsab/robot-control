/*
	Nödstopp på MP101:s DI1 (J18/J27), läst direkt av styrkortet.

	Byggs bara med NODSTOPP_DI1 (make ... BOARD=mp101 NODSTOPP=di1). Utan flaggan
	blir filen tom och firmwaren exakt som förut, också på MP101: utan knapp och
	pull-up flyter ingången.

	Koppling: NC-knapp mellan DI1 stift 2 (signal) och stift 3 (GND), och en pull-up
	till stift 1 (+5 V): R130 (1 kΩ, ej monterad från Upwis) eller ett motstånd i
	knappens plintar. Sluten knapp = låg = OK. Intryckt eller kabelbrott = hög = STOPP.
	DI1 är annars PWM-utgång (SERV_0, PB0); med flaggan vänds U34 (DIR låg) till ingång.

	Samma beteende som Car_Clients GPIO-nödstopp (nodstopp.cpp), men oberoende av Pi:n:
	- två höga läsningar i rad (40 ms) stoppar: autopiloten av, fart 0, hydraulik stopp,
	  och körkommandon släpps inte igenom;
	- utdragen knapp (låg i 0,5 s) ger "spärrad": körningen släpps först när föraren
	  skickar ett stoppkommando (spaken i neutral, Lås/Stopp), så roboten aldrig börjar
	  köra av sig själv;
	- läget skickas som [id][143][läge] varje sekund och vid ändring (0 ok, 1 intryckt,
	  2 spärrad), samma paket som Car_Client, så RControlStation visar det.
 */

#include "nodstopp_di.h"

#ifdef NODSTOPP_DI1

#ifndef IS_ROVMCU
#error "NODSTOPP_DI1 finns bara på MP101 (BOARD=mp101)"
#endif

#include "ch.h"
#include "hal.h"
#include "conf_general.h"
#include "datatypes.h"
#include "commands.h"
#include "autopilot.h"
#include "motor_control.h"
#include "buffer.h"

#if SERVO_VESC_ID < 0
#error "NODSTOPP_DI1: maskinen styr med PWM-servo på PB0 (DI1), välj en maskin med VESC-styrning"
#endif

#define NS_DI1_GPIO			GPIOB	// DI1 = SERV_0 = PB0 via U34
#define NS_DI1_PIN			0
#define NS_CMD_STATUS		143		// samma som Car_Clients nödstoppsstatus
#define NS_AKTIVITET_FART	10
#define NS_PERIOD_MS		20
#define NS_UTDRAGEN_MS		500

typedef enum {
	NS_OK = 0,
	NS_INTRYCKT = 1,
	NS_SPARRAD = 2,
} NS_LAGE;

static volatile NS_LAGE m_lage = NS_SPARRAD;	// börja spärrat, som Car_Client
static THD_WORKING_AREA(nodstopp_thread_wa, 512);
static THD_FUNCTION(nodstopp_thread, arg);

static void skicka_lage(void) {
	unsigned char buf[3];
	buf[0] = main_id;
	buf[1] = NS_CMD_STATUS;
	buf[2] = (unsigned char)m_lage;
	commands_send_packet(buf, 3);
}

static void satt_lage(NS_LAGE lage, const char *text) {
	m_lage = lage;
	commands_printf("%s", text);
	skicka_lage();
}

static void stoppa(void) {
	autopilot_set_active(false);
	motor_rc_adv(NS_AKTIVITET_FART, 0.0);
	motor_stop();
}

void nodstopp_di_init(void) {
	palClearPad(GPIOE, 4);	// DIR_U34 låg: DI1-kontakten -> STM32
	palSetPadMode(NS_DI1_GPIO, NS_DI1_PIN, PAL_MODE_INPUT);
	chThdCreateStatic(nodstopp_thread_wa, sizeof(nodstopp_thread_wa), NORMALPRIO + 1, nodstopp_thread, NULL);
}

static THD_FUNCTION(nodstopp_thread, arg) {
	(void)arg;
	chRegSetThreadName("Nodstopp DI1");

	int bruten_i_rad = 0;
	systime_t sluten_sedan = chVTGetSystemTimeX();
	systime_t senast_skickat = 0;
	systime_t senast_stoppat = 0;

	commands_printf("Nödstopp DI1: styrkortet startat, väntar på stoppkommando innan körning tillåts");

	for (;;) {
		const bool bruten = palReadPad(NS_DI1_GPIO, NS_DI1_PIN) != 0;

		if (bruten) {
			sluten_sedan = chVTGetSystemTimeX();
			if (++bruten_i_rad >= 2 && m_lage != NS_INTRYCKT) {
				satt_lage(NS_INTRYCKT, "NÖDSTOPP INTRYCKT (DI1) — körningen stoppad");
			}
		} else {
			bruten_i_rad = 0;
			if (m_lage == NS_INTRYCKT && chVTTimeElapsedSinceX(sluten_sedan) > MS2ST(NS_UTDRAGEN_MS)) {
				satt_lage(NS_SPARRAD, "Nödstopp utdraget. Släpp spaken / tryck Lås eller Stopp innan du kör igen");
			}
		}

		if (m_lage == NS_INTRYCKT && chVTTimeElapsedSinceX(senast_stoppat) >= MS2ST(200)) {
			stoppa();
			senast_stoppat = chVTGetSystemTimeX();
		}

		if (chVTTimeElapsedSinceX(senast_skickat) >= MS2ST(1000)) {
			skicka_lage();
			senast_skickat = chVTGetSystemTimeX();
		}

		chThdSleepMilliseconds(NS_PERIOD_MS);
	}
}

bool nodstopp_di_sparrar(uint8_t packet_id, const unsigned char *data, unsigned int len) {
	if (m_lage == NS_OK) {
		return false;
	}

	bool stopp = false;		// kommandot betyder "stå still"
	bool korning = false;	// kommandot kan få roboten att röra sig
	int32_t ind = 1;

	switch (packet_id) {
	case CMD_AP_SET_ACTIVE:
		stopp = len >= 1 && data[0] == 0;
		korning = !stopp;
		break;
	case CMD_RC_CONTROL_ADV:
		// Bara "fart = 0" släpps igenom.
		stopp = len >= 5 && data[0] == NS_AKTIVITET_FART && buffer_get_int32(data, &ind) == 0;
		korning = !stopp;
		break;
	case CMD_RC_CONTROL:
		// RControlStations Stopp: broms eller gas 0. Spärras medan knappen är intryckt
		// (kommandot styr även), men räknas som stoppkommando efteråt.
		stopp = len >= 5 && (data[0] == RC_MODE_CURRENT_BRAKE || buffer_get_int32(data, &ind) == 0);
		korning = true;
		break;
	case CMD_EMERGENCY_STOP:
		stopp = true;
		break;
	case CMD_IO_BOARD_SET_PWM_DUTY:
	case CMD_IO_BOARD_SET_VALVE:
	case CMD_HYDRAULIC_MOVE:
	case CMD_SET_SERVO_DIRECT:
	case CMD_STATE_CONTROL_ENABLE:
	case CMD_STATE_CONTROL_TARGET:
	case CMD_TEST_SPIN_VESC:
		korning = true;
		break;
	default:
		break;
	}

	if (m_lage == NS_SPARRAD && stopp) {
		satt_lage(NS_OK, "Nödstopp: spärren släppt, körning tillåten");
		return false;
	}

	return korning;
}

#endif	// NODSTOPP_DI1

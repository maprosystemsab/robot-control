/*
	Lägesknapp på MP101:s DI2 (J23/J32) och lampa på DI3 (J24/J37), för robotar med både
	RCS/Robotstyrning och SentinAnt: knappen hålls in 3 s för att byta läge (autorobot
	LAGESKNAPP_PLAN.md). Styrkortet byter inget läge själv, det rapporterar bara knappen.

	Byggs bara med LAGESKNAPP_DI2 (make ... BOARD=mp101 LAGESKNAPP=di2). Utan flaggan blir
	filen tom och firmwaren exakt som förut.

	Knappen (NO) kopplas mellan DI2 stift 2 (signal) och stift 3 (GND), med en pull-up på
	1–4,7 kΩ till stift 1 (+5 V): R129 är inte monterad. Intryckt = låg.
	DI2 är annars PWM-utgång (SERV_1, PB1); med flaggan vänds U38 (DIR_U38 = PB3 låg) till ingång.
	DI3 (PA2, U39 riktning ut) driver lampan som vanlig utgång i stället för PWM. Den går via
	220 Ω (R77): en lysdiod direkt, större last via en transistor.

	- Avstudsning 30 ms. Hållen intryckt i 3 s ger läge 2, en gång per tryck (tills den släpps).
	- [id][144][läge] skickas vid ändring och varje sekund (0 släppt, 1 intryckt, 2 hållen 3 s).
	- [id][145][läge] styr lampan: 0 av, 1 på, 2 blink (4 Hz). Utan 145 på 5 s blinkar den
	  långsamt (0,5 Hz): ingen styr kortet.
 */

#include "lagesknapp_di.h"

#ifdef LAGESKNAPP_DI2

#ifndef IS_ROVMCU
#error "LAGESKNAPP_DI2 finns bara på MP101 (BOARD=mp101)"
#endif

#include "ch.h"
#include "hal.h"
#include "conf_general.h"
#include "commands.h"

#define LK_GPIO			GPIOB	// DI2 = SERV_1 = PB1 via U38
#define LK_PIN			1
#define LAMPA_GPIO		GPIOA	// DI3 = SERV_2 = PA2 via U39
#define LAMPA_PIN		2
#define LK_PERIOD_MS	10
#define LK_AVSTUDS_MS	30
#define LK_HALL_MS		3000
#define LAMPA_TIMEOUT_MS	5000

static volatile uint8_t m_lampa = 0;
static volatile systime_t m_lampa_senast = 0;
static THD_WORKING_AREA(lagesknapp_thread_wa, 512);
static THD_FUNCTION(lagesknapp_thread, arg);

static void skicka_lage(uint8_t lage) {
	unsigned char buf[3];
	buf[0] = main_id;
	buf[1] = LAGESKNAPP_CMD_LAGE;
	buf[2] = lage;
	commands_send_packet(buf, 3);
}

void lagesknapp_di_lampa(uint8_t lage) {
	m_lampa = lage;
	m_lampa_senast = chVTGetSystemTimeX();
}

void lagesknapp_di_init(void) {
	palClearPad(GPIOB, 3);	// DIR_U38 låg: DI2-kontakten -> STM32
	palSetPadMode(LK_GPIO, LK_PIN, PAL_MODE_INPUT);
	palClearPad(LAMPA_GPIO, LAMPA_PIN);
	palSetPadMode(LAMPA_GPIO, LAMPA_PIN, PAL_MODE_OUTPUT_PUSHPULL);	// U39 är redan riktad ut
	m_lampa_senast = chVTGetSystemTimeX();
	chThdCreateStatic(lagesknapp_thread_wa, sizeof(lagesknapp_thread_wa), NORMALPRIO, lagesknapp_thread, NULL);
}

static THD_FUNCTION(lagesknapp_thread, arg) {
	(void)arg;
	chRegSetThreadName("Lagesknapp DI2");

	bool ra = false;			// senaste råa läsning (intryckt)
	bool intryckt = false;		// avstudsat läge
	systime_t ra_sedan = chVTGetSystemTimeX();
	systime_t intryckt_sedan = 0;
	uint8_t lage = 0;
	systime_t senast_skickat = 0;

	for (;;) {
		const systime_t nu = chVTGetSystemTimeX();
		const bool las = palReadPad(LK_GPIO, LK_PIN) == 0;	// låg = intryckt

		if (las != ra) {
			ra = las;
			ra_sedan = nu;
		}
		if (ra != intryckt && chVTTimeElapsedSinceX(ra_sedan) >= MS2ST(LK_AVSTUDS_MS)) {
			intryckt = ra;
			if (intryckt) {
				intryckt_sedan = nu;
			}
		}

		uint8_t nytt = intryckt ? 1 : 0;
		if (intryckt && (lage == 2 || chVTTimeElapsedSinceX(intryckt_sedan) >= MS2ST(LK_HALL_MS))) {
			nytt = 2;	// kvar på 2 tills knappen släpps: en händelse per tryck
		}
		if (nytt != lage || chVTTimeElapsedSinceX(senast_skickat) >= MS2ST(1000)) {
			lage = nytt;
			skicka_lage(lage);
			senast_skickat = nu;
		}

		// Lampan: 0 av, 1 på, 2 blink 4 Hz; utan kommando på 5 s långsam blink 0,5 Hz.
		const uint32_t ms = ST2MS(nu);
		bool tand;
		if (chVTTimeElapsedSinceX(m_lampa_senast) > MS2ST(LAMPA_TIMEOUT_MS)) {
			tand = (ms % 2000) < 1000;
		} else if (m_lampa == 2) {
			tand = (ms % 250) < 125;
		} else {
			tand = m_lampa == 1;
		}
		palWritePad(LAMPA_GPIO, LAMPA_PIN, tand ? 1 : 0);

		chThdSleepMilliseconds(LK_PERIOD_MS);
	}
}

#endif	// LAGESKNAPP_DI2

/*
	Copyright 2016 - 2017 Benjamin Vedder	benjamin@vedder.se

	This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

#include <math.h>
#include "adconv.h"
#include "utils.h"
#include "conf_general.h"
#include "terminal.h"
#include "commands.h"
#ifdef IS_ROVMCU
#include "pos.h"
#endif
#include "comm_can.h"

// Settings
#define VREFINT					1.21

#ifdef IS_ROVMCU
#define ADC_GRP_NUM_CHANNELS    6
#define ADC_VREF_INDEX          4
#else
#define ADC_GRP_NUM_CHANNELS	8
#define ADC_VREF_INDEX          6
#endif
#define ADC_GRP_BUF_DEPTH		1

static adcsample_t samples[ADC_GRP_NUM_CHANNELS * ADC_GRP_BUF_DEPTH];
static float vin_filter = 0.0;

static void terminal_cmd_get_vin(int argc, const char **argv);

#ifdef ANGLE_SENSOR_PA3
// Vinkelgivaren på PA3 (ADC-kanal 3) ligger på plats 6 i sekvensen (samples[5]),
// som annars var en dubblett av IN11.
#define ANGLE_SAMPLE_INDEX		5
extern float frontangle;
extern uint16_t last_sensorvalue;
static float angle_mv_filter = 0.0;
static bool angle_sensor_ok = false;
static void terminal_cmd_angle(int argc, const char **argv);
static void terminal_cmd_angle_zero(int argc, const char **argv);
static void terminal_cmd_angle_right(int argc, const char **argv);
static void terminal_cmd_angle_left(int argc, const char **argv);
#endif

static void adccallback(ADCDriver *adcp, adcsample_t *buffer, size_t n) {
	(void)adcp;
	(void)buffer;
	(void)n;

#ifdef IS_ROVMCU
	if (samples[ADC_VREF_INDEX] == 0) {
		return;
	}
	const float v_reg = (VREFINT * 4095.0) / (float)samples[ADC_VREF_INDEX];
	float sample = (samples[0] / 4095.0 * v_reg) * ((PWR_5V_R1 + PWR_5V_R2) / PWR_5V_R2);
#else
	const float v_reg = (VREFINT * 4095.0) / (float)samples[ADC_VREF_INDEX];
	float sample = (samples[0] / 4095.0 * v_reg) * ((VIN_R1 + VIN_R2) / VIN_R2);
#endif
	UTILS_LP_FAST(vin_filter, sample, 0.02);

#ifdef ANGLE_SENSOR_PA3
	// Givarens egen spänning (före spänningsdelaren) i mV
	float angle_mv = (samples[ANGLE_SAMPLE_INDEX] / 4095.0 * v_reg) * ANGLE_SENSOR_DIVIDER * 1000.0;
	UTILS_LP_FAST(angle_mv_filter, angle_mv, 0.02);
#endif
}

//static void adcerrorcallback(ADCDriver *adcp, adcerror_t err) {
//	(void)adcp;
//	(void)err;
//}

/*
 * ADC conversion group.
 * Mode:        Continuous, 16 samples of 8 channels, SW triggered.
 */
static const ADCConversionGroup adcgrpcfg = {
		TRUE,
		ADC_GRP_NUM_CHANNELS,
		adccallback,
		0,//adcerrorcallback,
		0,                        /* CR1 */
		ADC_CR2_SWSTART,          /* CR2 */
#ifdef IS_ROVMCU
		ADC_SMPR1_SMP_AN11(ADC_SAMPLE_56) |
		ADC_SMPR1_SMP_SENSOR(ADC_SAMPLE_144) |
		ADC_SMPR1_SMP_VREF(ADC_SAMPLE_144),
		ADC_SMPR2_SMP_AN4(ADC_SAMPLE_56) |
		ADC_SMPR2_SMP_AN5(ADC_SAMPLE_56) |
		ADC_SMPR2_SMP_AN6(ADC_SAMPLE_56),
		ADC_SQR1_NUM_CH(ADC_GRP_NUM_CHANNELS),
		0,
		ADC_SQR3_SQ6_N(ADC_CHANNEL_SENSOR) | ADC_SQR3_SQ5_N(ADC_CHANNEL_VREFINT) |
		ADC_SQR3_SQ4_N(ADC_CHANNEL_IN6) | ADC_SQR3_SQ3_N(ADC_CHANNEL_IN5) |
		ADC_SQR3_SQ2_N(ADC_CHANNEL_IN4) | ADC_SQR3_SQ1_N(ADC_CHANNEL_IN11)
#else
		ADC_SMPR1_SMP_AN10(ADC_SAMPLE_56) |
		ADC_SMPR1_SMP_AN11(ADC_SAMPLE_56) |
		ADC_SMPR1_SMP_AN12(ADC_SAMPLE_56) |
		ADC_SMPR1_SMP_AN13(ADC_SAMPLE_56) |
		ADC_SMPR1_SMP_SENSOR(ADC_SAMPLE_144) |
		ADC_SMPR1_SMP_VREF(ADC_SAMPLE_144),
#ifdef ANGLE_SENSOR_PA3
		ADC_SMPR2_SMP_AN4(ADC_SAMPLE_56) |
		ADC_SMPR2_SMP_AN3(ADC_SAMPLE_144), /* SMPR2 */
#else
		ADC_SMPR2_SMP_AN4(ADC_SAMPLE_56), /* SMPR2 */
#endif
		ADC_SQR1_NUM_CH(ADC_GRP_NUM_CHANNELS),
		ADC_SQR2_SQ8_N(ADC_CHANNEL_SENSOR) | ADC_SQR2_SQ7_N(ADC_CHANNEL_VREFINT),
#ifdef ANGLE_SENSOR_PA3
		ADC_SQR3_SQ6_N(ADC_CHANNEL_IN3)   | ADC_SQR3_SQ5_N(ADC_CHANNEL_IN10) |
#else
		ADC_SQR3_SQ6_N(ADC_CHANNEL_IN11)  | ADC_SQR3_SQ5_N(ADC_CHANNEL_IN10) |
#endif
		ADC_SQR3_SQ4_N(ADC_CHANNEL_IN13) | ADC_SQR3_SQ3_N(ADC_CHANNEL_IN12) |
		ADC_SQR3_SQ2_N(ADC_CHANNEL_IN11) | ADC_SQR3_SQ1_N(ADC_CHANNEL_IN10)
#endif
};

void adconv_init(void) {
#ifdef IS_ROVMCU
	// PC0/PC2/PC3 have no populated ADC input path. J30 ADC11-13 are PA4-6.
	palSetPadMode(GPIOC, 1, PAL_MODE_INPUT_ANALOG);
	palSetPadMode(GPIOA, 4, PAL_MODE_INPUT_ANALOG);
	palSetPadMode(GPIOA, 5, PAL_MODE_INPUT_ANALOG);
	palSetPadMode(GPIOA, 6, PAL_MODE_INPUT_ANALOG);
#else
	palSetPadMode(GPIOC, 0, PAL_MODE_INPUT_ANALOG);
	palSetPadMode(GPIOC, 1, PAL_MODE_INPUT_ANALOG);
	palSetPadMode(GPIOC, 2, PAL_MODE_INPUT_ANALOG);
	palSetPadMode(GPIOC, 3, PAL_MODE_INPUT_ANALOG);
#ifdef ANGLE_SENSOR_PA3
	palSetPadMode(GPIOA, 3, PAL_MODE_INPUT_ANALOG);
#endif
#endif

	adcStart(&ADCD1, NULL);
	adcSTM32EnableTSVREFE();

	adcStartConversion(&ADCD1, &adcgrpcfg, samples, ADC_GRP_BUF_DEPTH);

	terminal_register_command_callback(
#ifdef IS_ROVMCU
			"adconv_get_5v",
			"Read the regulated 5 V rail (not battery voltage).",
#else
			"adconv_get_vin",
			"Read the input voltage.",
#endif
			0,
			terminal_cmd_get_vin);

#ifdef ANGLE_SENSOR_PA3
	terminal_register_command_callback(
			"vinkel",
			"Vinkelgivaren: spänning (mV), vinkel och status.",
			0,
			terminal_cmd_angle);
	terminal_register_command_callback(
			"vinkel_noll",
			"Kalibrera vinkelgivaren: hjulen rakt fram nu. Sparas i styrkortet.",
			0,
			terminal_cmd_angle_zero);
	terminal_register_command_callback(
			"vinkel_hoger",
			"Kalibrera vinkelgivaren: hjulen fullt höger nu. Sparas i styrkortet.",
			0,
			terminal_cmd_angle_right);
	terminal_register_command_callback(
			"vinkel_vanster",
			"Kalibrera vinkelgivaren: hjulen fullt vänster nu. Sparas i styrkortet.",
			0,
			terminal_cmd_angle_left);
#endif
}

/**
 * Get the raw ADC value from one of the pins on the ADC_GPIO header.
 */
uint16_t adconv_get_pin(int pin) {
	if (pin < 0 || pin >= 4) {
		return 0;
	}

	return samples[pin];
}

/**
 * Read the voltage on ADC input pin.
 *
 * @param pin
 * The pin to read from.
 *
 * @return
 * The voltage in volts.
 */
float adconv_get_volts(int pin) {
#ifdef IS_ROVMCU
	if (samples[ADC_VREF_INDEX] == 0) {
		return 0.0f;
	}
#endif
	const float v_reg = (VREFINT * 4095.0) / (float)samples[ADC_VREF_INDEX];
	return (float)adconv_get_pin(pin) / 4095.0 * v_reg;
}

/**
 * Get battery voltage: filtered ADC on legacy boards, VESC/CAN on MP101.
 *
 * @return
 * Battery voltage in volts (zero on MP101 until the first CAN reading).
 */
float adconv_get_vin(void) {
#ifdef IS_ROVMCU
	// Keep all battery-voltage consumers on the same VESC/CAN source used by
	// CMD_GET_STATE. Zero before the first reply means no battery reading yet.
	mc_values values;
	pos_get_mc_val(&values);
	return values.v_in;
#else
	return vin_filter;
#endif
}

#ifdef IS_ROVMCU
float adconv_get_5v(void) {
	return vin_filter;
}
#endif

static void terminal_cmd_get_vin(int argc, const char **argv) {
	(void)argc;
	(void)argv;
#ifdef IS_ROVMCU
	commands_printf("Regulated 5 V rail: %.2f V\n", (double)adconv_get_5v());
#else
	commands_printf("Input Voltage: %.2f V\n", (double)adconv_get_vin());
#endif
}

#ifdef ANGLE_SENSOR_PA3
/**
 * Givarens spänning rakt fram (mV). Kalibreras med `vinkel_noll` och sparas i
 * main_config.vehicle.sensorcentre; orimligt värde (t.ex. standard 580 från
 * äldre maskiner) -> ANGLE_SENSOR_CENTER_MV.
 */
static float angle_center_mv(void) {
	float c = main_config.vehicle.sensorcentre;
	return (c > 500.0 && c < 4500.0 && c != 580.0) ? c : ANGLE_SENSOR_CENTER_MV;
}

/**
 * Trepunktskalibrering (vinkel_noll, vinkel_hoger, vinkel_vanster). Ändlägena
 * sparas i sensorinterval (höger) och degreeinterval (vänster), som inte används
 * för vinkelgivaren på PA3. Giltig om båda ändlägena ligger på var sin sida om
 * mitten; annars används den linjära ANGLE_SENSOR_DEG_PER_MV.
 */
static bool angle_cal_mv(float *c, float *r, float *l) {
	*c = angle_center_mv();
	*r = main_config.vehicle.sensorinterval;
	*l = main_config.vehicle.degreeinterval;
	bool in_range = *r > 100.0 && *r < 4900.0 && *l > 100.0 && *l < 4900.0;
	return in_range && fabsf(*r - *c) > 50.0 && fabsf(*l - *c) > 50.0 && (*r - *c) * (*l - *c) < 0.0;
}

/**
 * Räkna om vinkelgivarens spänning till vinkel. Anropas var 10:e ms från main.
 * Vid givarfel (spänning nära 0 V: magneten saknas eller kabeln av) behålls
 * senaste vinkeln och statusen blir "fel".
 */
void adconv_update_angle(void) {
	float mv = angle_mv_filter;
	last_sensorvalue = (uint16_t)(mv < 0.0 ? 0.0 : mv);	// mV, skickas med i statusen

	if (mv < ANGLE_SENSOR_MIN_MV) {
		angle_sensor_ok = false;
		return;
	}

	angle_sensor_ok = true;
	float c, r, l;
	if (angle_cal_mv(&c, &r, &l)) {
		// Höger = +ANGLE_SENSOR_MAX_DEG, vänster = -ANGLE_SENSOR_MAX_DEG, linjärt på var sida.
		if ((mv - c) * (r - c) >= 0.0) {
			frontangle = (mv - c) / (r - c) * ANGLE_SENSOR_MAX_DEG;
		} else {
			frontangle = -(mv - c) / (l - c) * ANGLE_SENSOR_MAX_DEG;
		}
	} else {
		frontangle = (mv - c) * ANGLE_SENSOR_DEG_PER_MV;
	}
	comm_can_io_board_as5047_setangle(frontangle);
}

bool adconv_angle_sensor_ok(void) {
	return angle_sensor_ok;
}

static void terminal_cmd_angle(int argc, const char **argv) {
	(void)argc;
	(void)argv;
	float c, r, l;
	bool cal = angle_cal_mv(&c, &r, &l);
	if (cal) {
		commands_printf("Vinkelgivare: %.0f mV, vinkel %.1f grader, %s (rakt fram %.0f mV, höger %.0f mV, vänster %.0f mV)\n",
				(double)angle_mv_filter, (double)frontangle,
				angle_sensor_ok ? "OK" : "FEL (ingen magnet/kabel?)",
				(double)c, (double)r, (double)l);
	} else {
		commands_printf("Vinkelgivare: %.0f mV, vinkel %.1f grader, %s (rakt fram %.0f mV, höger/vänster ej kalibrerat)\n",
				(double)angle_mv_filter, (double)frontangle,
				angle_sensor_ok ? "OK" : "FEL (ingen magnet/kabel?)",
				(double)c);
	}
}

static void terminal_cmd_angle_zero(int argc, const char **argv) {
	(void)argc;
	(void)argv;
	if (!angle_sensor_ok) {
		commands_printf("Vinkelgivaren svarar inte (ingen magnet/kabel?) - inget sparat.\n");
		return;
	}
	main_config.vehicle.sensorcentre = angle_mv_filter;
	conf_general_store_main_config(&main_config);
	commands_printf("Vinkelgivaren kalibrerad: rakt fram = %.0f mV. Sparat i styrkortet.\n",
			(double)angle_mv_filter);
}

static void angle_store_end(bool right) {
	if (!angle_sensor_ok) {
		commands_printf("Vinkelgivaren svarar inte (ingen magnet/kabel?) - inget sparat.\n");
		return;
	}
	float c = angle_center_mv();
	if (fabsf(angle_mv_filter - c) <= 50.0) {
		commands_printf("Hjulen står nästan rakt (%.0f mV, rakt fram %.0f mV). Styr fullt %s och försök igen.\n",
				(double)angle_mv_filter, (double)c, right ? "höger" : "vänster");
		return;
	}
	if (right) {
		main_config.vehicle.sensorinterval = angle_mv_filter;
	} else {
		main_config.vehicle.degreeinterval = angle_mv_filter;
	}
	conf_general_store_main_config(&main_config);
	commands_printf("Vinkelgivaren kalibrerad: fullt %s = %.0f mV (%s%.0f grader). Sparat i styrkortet.\n",
			right ? "höger" : "vänster", (double)angle_mv_filter, right ? "" : "-", (double)ANGLE_SENSOR_MAX_DEG);
}

static void terminal_cmd_angle_right(int argc, const char **argv) {
	(void)argc;
	(void)argv;
	angle_store_end(true);
}

static void terminal_cmd_angle_left(int argc, const char **argv) {
	(void)argc;
	(void)argv;
	angle_store_end(false);
}
#endif

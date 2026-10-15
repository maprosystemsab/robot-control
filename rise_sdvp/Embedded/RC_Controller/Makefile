##############################################################################
# Build global options
# NOTE: Can be overridden externally.
#

# All boards share build/. Run make clean before changing board or profile.
# Example: make clean && make -j4 robant BOARD=mp101
BOARD ?= legacy
ifeq ($(BOARD),mp101)
  MCU_DEFS = -DSTM32F415xx -DIS_ROVMCU
else ifeq ($(BOARD),legacy)
  MCU_DEFS = -DSTM32F405xx
else
  $(error BOARD must be legacy or mp101)
endif

# Compiler options here.
ifeq ($(USE_OPT),)
  USE_OPT = -O2 -ggdb -fomit-frame-pointer -falign-functions=16 -std=gnu99
  USE_OPT += -DBOARD_OTG_NOVBUSSENS $(MCU_DEFS) $(build_args)
  USE_OPT += -fsingle-precision-constant -Wdouble-promotion
  USE_OPT += --specs=nosys.specs
endif

# C specific options here (added to USE_OPT).
ifeq ($(USE_COPT),)
  USE_COPT = 
endif

# C++ specific options here (added to USE_OPT).
ifeq ($(USE_CPPOPT),)
  USE_CPPOPT = -fno-rtti
  #Newlib
  USE_CPPOPT += --specs=nosys.specs
#  USE_CPPOPT = -nostdlib
endif

# Enable this if you want the linker to remove unused code and data
ifeq ($(USE_LINK_GC),)
  USE_LINK_GC = yes
endif

# Linker extra options here.
ifeq ($(USE_LDOPT),)
  USE_LDOPT = 
#  USE_LDOPT = --specs=nano.specs
#  USE_LDOPT = -nostdlib
endif

# Enable this if you want link time optimizations (LTO)
ifeq ($(USE_LTO),)
  USE_LTO = no
endif

# If enabled, this option allows to compile the application in THUMB mode.
ifeq ($(USE_THUMB),)
  USE_THUMB = yes
endif

# Enable this if you want to see the full log while compiling.
ifeq ($(USE_VERBOSE_COMPILE),)
  USE_VERBOSE_COMPILE = yes
endif

# If enabled, this option makes the build process faster by not compiling
# modules not used in the current configuration.
ifeq ($(USE_SMART_BUILD),)
  USE_SMART_BUILD = yes
endif


HAL_COMPONENTS = ICU

#
# Build global options
##############################################################################

##############################################################################
# Architecture or project specific options
#

# Stack size to be allocated to the Cortex-M process stack. This stack is
# the stack used by the main() thread.
ifeq ($(USE_PROCESS_STACKSIZE),)
  USE_PROCESS_STACKSIZE = 0x400
endif

# Stack size to the allocated to the Cortex-M main/exceptions stack. This
# stack is used for processing interrupts and exceptions.
ifeq ($(USE_EXCEPTIONS_STACKSIZE),)
  USE_EXCEPTIONS_STACKSIZE = 0x400
endif

# Enables the use of FPU on Cortex-M4 (no, softfp, hard).
ifeq ($(USE_FPU),)
  USE_FPU = hard
  USE_OPT += -mfpu=fpv4-sp-d16 -mfloat-abi=hard
endif

# Enable this if you really want to use the STM FWLiarm-none-eabi-gcc -c -mcpu=cortex-m4 -O2 -ggdb -fomit-frame-pointer -falign-functions=16 -std=gnu99 -DBOARD_OTG_NOVBUSSENS -DSTM32F405xx  -fsingle-precision-constant -Wdouble-promotion --specs=nosys.specs -mfpu=fpv4-sp-d16 -mfloat-abi=hard -DUSE_STDPERIPH_DRIVER -ffunction-sections -fdata-sections -fno-common -mfloat-abi=hard -mfpu=fpv4-sp-d16 -fsingle-precision-constant  -Wall -Wextra -Wundef -Wstrict-prototypes -Wa,-alms=build/lst/hydraulic.lst -DCORTEX_USE_FPU=TRUE  -DTHUMB_PRESENT -mno-thumb-interwork -DTHUMB_NO_INTERWORKING -MD -MP -MF .dep/hydraulic.o.d -mthumb -DTHUMB -I. -IChibiOS_3.0.5/os/common/ports/ARMCMx/devices/STM32F4xx -IChibiOS_3.0.5/os/ext/CMSIS/include -IChibiOS_3.0.5/os/ext/CMSIS/ST -IChibiOS_3.0.5/os/rt/include -IChibiOS_3.0.5/os/rt/ports/ARMCMx -IChibiOS_3.0.5/os/rt/ports/ARMCMx/compilers/GCC -IChibiOS_3.0.5/os/hal/osal/rt -IChibiOS_3.0.5/os/hal/include -IChibiOS_3.0.5/os/hal/ports/common/ARMCMx -IChibiOS_3.0.5/os/hal/ports/STM32/STM32F4xx -IChibiOS_3.0.5/os/hal/ports/STM32/LLD -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/DACv1 -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/GPIOv2 -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/I2Cv1 -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/OTGv1 -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/RTCv2 -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/SPIv1 -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/TIMv1 -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/USARTv1 -IChibiOS_3.0.5/os/hal/ports/STM32/LLD/FSMCv1 -IChibiOS_3.0.5/os/hal/boards/ST_STM32F4_DISCOVERY -IChibiOS_3.0.5/os/various -IChibiOS_3.0.5/os/hal/lib/streams -Icc2520/driver -Icc2520/basicrf -Icc1120 -Iimu -Iimu/BMI160_driver -IChibiOS_3.0.5/os/hal/include -IChibiOS_3.0.5/ext/stdperiph_stm32f4/inc hydraulic.c -o build/obj/hydraulic.ob.
ifeq ($(USE_FWLIB),)
  USE_FWLIB = yes
endif

#
# Architecture or project specific options
##############################################################################

##############################################################################
# Project, sources and paths
#

# Define project name here
PROJECT = rc_controller

# Imported source files and paths
CHIBIOS = ChibiOS_3.0.5
# Startup files.
include $(CHIBIOS)/os/common/ports/ARMCMx/compilers/GCC/mk/startup_stm32f4xx.mk
# HAL-OSAL files (optional).
include $(CHIBIOS)/os/hal/hal.mk
include $(CHIBIOS)/os/hal/ports/STM32/STM32F4xx/platform.mk
ifeq ($(BOARD),mp101)
include boards/MP101_323/board.mk
else
include $(CHIBIOS)/os/hal/boards/ST_STM32F4_DISCOVERY/board.mk
endif
include $(CHIBIOS)/os/hal/osal/rt/osal.mk
# RTOS files (optional).
include $(CHIBIOS)/os/rt/rt.mk
include $(CHIBIOS)/os/rt/ports/ARMCMx/compilers/GCC/mk/port_v7m.mk
# Other files (optional).
include imu/imu.mk

# Define linker script file here
LDSCRIPT= ld_eeprom_emu.ld

# C sources that can be compiled in ARM or THUMB mode depending on the global
# setting.
CSRC = $(STARTUPSRC) \
       $(KERNSRC) \
       $(PORTSRC) \
       $(OSALSRC) \
       $(HALSRC) \
       $(PLATFORMSRC) \
       $(BOARDSRC) \
       $(CHIBIOS)/os/hal/lib/streams/chprintf.c \
       $(CHIBIOS)/os/various/syscalls.c \
       $(IMUSRC) \
       main.c \
       comm_usb_serial.c \
       utils.c \
       led.c \
       buffer.c \
       ext_cb.c \
       crc.c \
       adconv.c \
       comm_usb.c \
       packet.c \
       commands.c \
       conf_general.c \
       pos.c \
       terminal.c \
       comm_can.c \
       bldc_interface.c \
       servo_simple.c \
       autopilot.c \
       eeprom.c \
       timeout.c \
       log.c \
       digital_filter.c \
       ublox.c \
       rtcm3_simple.c \
       srf10.c \
       pwm_esc.c \
       actuator.c \
       motor_sim.c \
       m8t_base.c \
       pos_uwb.c \
       fi.c \
       servo_vesc.c \
       hydraulic.c \
       i2c_bb.c \
       timer.c \
       wheelspeed.c \
       motor_control.c \
       steering_control.c \
       styr_position.c \
       actions.c \
       sensor_control.c \
       state_control.c \
       test_sensor_state_control.c

# C++ sources that can be compiled in ARM or THUMB mode depending on the global
# setting.
CPPSRC =

# C sources to be compiled in ARM mode regardless of the global setting.
# NOTE: Mixing ARM and THUMB mode enables the -mthumb-interwork compiler
#       option that results in lower performance and larger code size.
ACSRC =

# C++ sources to be compiled in ARM mode regardless of the global setting.
# NOTE: Mixing ARM and THUMB mode enables the -mthumb-interwork compiler
#       option that results in lower performance and larger code size.
ACPPSRC =

# C sources to be compiled in THUMB mode regardless of the global setting.
# NOTE: Mixing ARM and THUMB mode enables the -mthumb-interwork compiler
#       option that results in lower performance and larger code size.
TCSRC =

# C sources to be compiled in THUMB mode regardless of the global setting.
# NOTE: Mixing ARM and THUMB mode enables the -mthumb-interwork compiler
#       option that results in lower performance and larger code size.
TCPPSRC =

# List ASM source files here
ASMSRC = $(STARTUPASM) $(PORTASM) $(OSALASM)

INCDIR = $(STARTUPINC) $(KERNINC) $(PORTINC) $(OSALINC) \
         $(HALINC) $(PLATFORMINC) $(BOARDINC) $(TESTINC) \
         $(CHIBIOS)/os/various \
         $(CHIBIOS)/os/hal/lib/streams \
         $(IMUINC) \
         $(CHIBIOS)/os/hal/include

#
# Project, sources and paths
##############################################################################

##############################################################################
# Compiler settings
#

MCU  = cortex-m4

#TRGT = arm-elf-
TRGT = arm-none-eabi-
CC   = $(TRGT)gcc
CPPC = $(TRGT)g++
# Enable loading with g++ only if you need C++ runtime support.
# NOTE: You can use C++ even without C++ support if you are careful. C++
#       runtime support makes code size explode.
LD   = $(TRGT)gcc
#LD   = $(TRGT)g++
CP   = $(TRGT)objcopy
AS   = $(TRGT)gcc -x assembler-with-cpp
AR   = $(TRGT)ar
OD   = $(TRGT)objdump
SZ   = $(TRGT)size
HEX  = $(CP) -O ihex
BIN  = $(CP) -O binary

$(info CC="$(CC)")
$(info LD="$(LD)")

# ARM-specific options here
AOPT =

# THUMB-specific options here
TOPT = -mthumb -DTHUMB

# Define C warning options here
CWARN = -Wall -Wextra -Wundef -Wstrict-prototypes

# Define C++ warning options here
CPPWARN = -Wall -Wextra -Wundef

#
# Compiler settings
##############################################################################

##############################################################################
# Start of user section
#

# List all user C define here, like -D_DEBUG=1
UDEFS =

# Define ASM defines here
UADEFS =

# List all user directories here
UINCDIR =

# List the user directory to look for the libraries here
ULIBDIR =

# List all user libraries here
ULIBS = -lm

#
# End of user defines
##############################################################################

ifeq ($(USE_FWLIB),yes)
  include $(CHIBIOS)/ext/stdperiph_stm32f4/stm32lib.mk
  CSRC += $(STM32SRC)
  INCDIR += $(STM32INC)
  USE_OPT += -DUSE_STDPERIPH_DRIVER
endif

build/$(PROJECT).bin: build/$(PROJECT).elf 
	$(BIN) build/$(PROJECT).elf build/$(PROJECT).bin

.PHONY: upload upload-olimex debug-start upload_remote

# Program
#upload: build/$(PROJECT).bin
#	openocd -f board/stm32f4discovery.cfg -c "reset_config trst_only combined" -c "program build/$(PROJECT).elf verify reset exit" # For openocd 0.9
#  openocd -d -f board/stm32f4discovery.cfg -c "reset_config trst_only combined" -c "program build/$(PROJECT).elf verify reset exit" # For openocd 0.9
upload: build/$(PROJECT).bin
	openocd -d -f board/stm32f4discovery.cfg -c "reset_config trst_only combined" -c "program build/$(PROJECT).elf verify reset exit"


# FOR SEGGER J_LINK
#	openocd -f interface/jlink.cfg -c "transport select swd" -f target/stm32f4x.cfg -c "adapter speed 2000" -c "program build/$(PROJECT).elf verify reset exit"

#program with olimex arm-usb-tiny-h and jtag-swd adapter board. needs openocd>=0.9
upload-olimex: build/$(PROJECT).bin
	openocd -f interface/ftdi/olimex-arm-usb-tiny-h.cfg -f interface/ftdi/olimex-arm-jtag-swd.cfg -c "set WORKAREASIZE 0x2000" -f target/stm32f4x.cfg -c "program build/$(PROJECT).elf verify reset"

debug-start:
#	openocd -f stm32-bv_openocd.cfg
	openocd -f stm32-bv_openocd.cfg -d
	
upload_remote: build/$(PROJECT).bin
#	./upload_remote build/$(PROJECT).bin nvidia 192.168.200.7 22 stlink jetson
#	./upload_remote build/$(PROJECT).bin pi 192.168.200.4 22 stlink raspberry
	./upload_remote build/$(PROJECT).bin pi 192.168.200.8 22 stlink raspberry

touch_conf_general:
	touch ./conf_general.h

vcc: override PROJECT = fw_vcc
vcc: USE_OPT += -DMAIN_MODE=MAIN_MODE_vehicle -DUBLOX_EN=0
vcc: touch_conf_general build/fw_vcc.elf build/fw_vcc.bin	./upload_remote build/$(PROJECT).bin pi 192.168.200.105 22 pi_gpio


rise_ubx: override PROJECT = fw_rise_ubx
rise_ubx: USE_OPT += -DMAIN_MODE=MAIN_MODE_vehicle -DUBLOX_EN=1 -DBOARD_YAW_ROT=-90.0
rise_ubx: touch_conf_general build/fw_rise_ubx.elf build/fw_rise_ubx.bin

rise_f9: override PROJECT = fw_rise_f9
rise_f9: USE_OPT += -DMAIN_MODE=MAIN_MODE_vehicle -DIS_F9_BOARD=1
rise_f9: touch_conf_general build/fw_rise_f9.elf build/fw_rise_f9.bin

drangen: override PROJECT = fw_drangen
drangen: BOARD_YAW_ROT ?= 180.0
drangen: USE_OPT += -DMAIN_MODE=MAIN_MODE_vehicle -DUBLOX_EN=1 -DBOARD_YAW_ROT=$(BOARD_YAW_ROT) -DHAS_HYDRAULIC_DRIVE=0 -DSERVO_VESC_ID=36
drangen: touch_conf_general build/fw_drangen.elf build/fw_drangen.bin

mactrac: override PROJECT = fw_mactrac
mactrac: USE_OPT += -DMAIN_MODE=MAIN_MODE_vehicle -DIS_MACTRAC
mactrac: touch_conf_general build/fw_mactrac.elf build/fw_mactrac.bin
ifeq ($(BOARD),mp101)
mactrac: BOARD_YAW_ROT ?= -90.0
mactrac: USE_OPT += -DBOARD_YAW_ROT=$(BOARD_YAW_ROT)
endif

robant: override PROJECT = fw_robant
robant: BOARD_YAW_ROT ?= 180.0
robant: USE_OPT += -DMAIN_MODE=MAIN_MODE_vehicle -DUBLOX_EN=1 -DBOARD_YAW_ROT=$(BOARD_YAW_ROT) -DHAS_HYDRAULIC_DRIVE=0 -DIS_ROBANT
robant: touch_conf_general build/fw_robant.elf build/fw_robant.bin

RULESPATH = $(CHIBIOS)/os/common/ports/ARMCMx/compilers/GCC
include $(RULESPATH)/rules.mk


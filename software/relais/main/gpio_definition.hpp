#include <cstdint>
#include <driver/gpio.h>

constexpr gpio_num_t TX_GPIO_NUM        = GPIO_NUM_13;
constexpr gpio_num_t RX_GPIO_NUM        = GPIO_NUM_14;
constexpr gpio_num_t ONEWIRE_GPIO_NUM   =  GPIO_NUM_4;
constexpr gpio_num_t SDA_GPIO_NUM       = GPIO_NUM_21;
constexpr gpio_num_t SCL_GPIO_NUM       = GPIO_NUM_19;
constexpr gpio_num_t EXT_BUTTON_GND     =  GPIO_NUM_2; //VCC     //GND
constexpr gpio_num_t EXT_BUTTON_VCC     =  GPIO_NUM_5; //SDA     //VCC
constexpr gpio_num_t EXT_SENSOR_VCC     =  GPIO_NUM_2; //VCC     //GND
constexpr gpio_num_t EXT_SENSOR_GND     = GPIO_NUM_18; //GND     //SW4
constexpr gpio_num_t EXT_SENSOR_OUT     = GPIO_NUM_17; //OUT     //SW3
constexpr gpio_num_t EXT_SENSOR_ONEWIRE = GPIO_NUM_16; //ONEWIRE //SW2
constexpr gpio_num_t EXT_SENSOR_SCL     = GPIO_NUM_15; //SCL     //SW1
constexpr gpio_num_t EXT_SENSOR_SDA     =  GPIO_NUM_5; //SDA     //VCC

constexpr gpio_num_t SW1_GPIO_NUM   = GPIO_NUM_33;
constexpr gpio_num_t SW2_GPIO_NUM   = GPIO_NUM_35;
constexpr gpio_num_t SW3_GPIO_NUM   = GPIO_NUM_12;
constexpr gpio_num_t SW4_GPIO_NUM   = GPIO_NUM_34;
constexpr gpio_num_t EXT_BUTTON_SW4 = GPIO_NUM_18; //GND     //SW4
constexpr gpio_num_t EXT_BUTTON_SW3 = GPIO_NUM_17; //OUT     //SW3
constexpr gpio_num_t EXT_BUTTON_SW2 = GPIO_NUM_16; //ONEWIRE //SW2
constexpr gpio_num_t EXT_BUTTON_SW1 = GPIO_NUM_15; //SCL     //SW1

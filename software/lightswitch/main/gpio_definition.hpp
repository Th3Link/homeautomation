#include <cstdint>
#include <driver/gpio.h>

constexpr gpio_num_t TX_GPIO_NUM        = GPIO_NUM_13;
constexpr gpio_num_t RX_GPIO_NUM        = GPIO_NUM_14;
constexpr gpio_num_t ONEWIRE_GPIO_NUM   = GPIO_NUM_27;
constexpr gpio_num_t DIM1_GPIO_NUM      = GPIO_NUM_21;
constexpr gpio_num_t DIM2_GPIO_NUM      = GPIO_NUM_19;
//constexpr gpio_num_t DIM3_GPIO_NUM      = GPIO_NUM_34;
constexpr gpio_num_t DIM4_GPIO_NUM      = GPIO_NUM_33;
constexpr gpio_num_t DIM5_GPIO_NUM      = GPIO_NUM_23;
constexpr gpio_num_t DIM6_GPIO_NUM      = GPIO_NUM_22;
//constexpr gpio_num_t DIM7_GPIO_NUM      = GPIO_NUM_35;
constexpr gpio_num_t DIM8_GPIO_NUM      = GPIO_NUM_32;

constexpr gpio_num_t EXT_SENSOR_OUT     = GPIO_NUM_18; //OUT     //SW3
constexpr gpio_num_t EXT_SENSOR_ONEWIRE = GPIO_NUM_17; //ONEWIRE //SW2
constexpr gpio_num_t EXT_SENSOR_SCL     = GPIO_NUM_16; //SCL     //SW1
constexpr gpio_num_t EXT_SENSOR_SDA     =  GPIO_NUM_4; //SDA     //VCC

constexpr gpio_num_t SW1_GPIO_NUM   = GPIO_NUM_25;
constexpr gpio_num_t SW2_GPIO_NUM   = GPIO_NUM_26;
constexpr gpio_num_t SW3_GPIO_NUM   =  GPIO_NUM_5;
constexpr gpio_num_t SW4_GPIO_NUM   = GPIO_NUM_15;
constexpr gpio_num_t EXT_BUTTON_SW4 = GPIO_NUM_18; //OUT     //SW4 //B4
constexpr gpio_num_t EXT_BUTTON_SW3 = GPIO_NUM_17; //ONEWIRE //SW3 //B3
constexpr gpio_num_t EXT_BUTTON_SW2 = GPIO_NUM_16; //SCL     //SW2 //B2
constexpr gpio_num_t EXT_BUTTON_SW1 =  GPIO_NUM_4; //SDA     //SW1 //B1

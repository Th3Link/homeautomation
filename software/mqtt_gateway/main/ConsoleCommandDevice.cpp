/* Console example — various system commands

   This example code is in the Public Domain (or CC0 licensed, at your option.)

   Unless required by applicable law or agreed to in writing, this
   software is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
   CONDITIONS OF ANY KIND, either express or implied.
*/

#include <stdio.h>
#include <string.h>
#include <ctype.h>
#include <inttypes.h>
#include <unistd.h>
#include <esp_log.h>
#include <esp_console.h>
#include <esp_flash.h>
#include <nvs_flash.h>
#include <esp_ota_ops.h>
#include <argtable3/argtable3.h>
#include "ConsoleCommandDevice.hpp"
#include "esp32-ha-lib/ICAN.hpp"

static const char *TAG = "ConsoleCommandDevice";

static void register_summary(void);
static void register_id(void);
static void register_type(void);
static void register_custom_string(void);
static void register_rev(void);
static void register_can_bitrate(void);
static struct {
    struct arg_int *id;
    struct arg_end *end;
} device_id_args;

static struct {
    struct arg_str *type;
    struct arg_end *end;
} device_type_args;

static struct {
    struct arg_str *custom_string;
    struct arg_end *end;
} device_custom_string_args;

static struct {
    struct arg_int *rev;
    struct arg_end *end;
} device_rev_args;

static struct {
    struct arg_int *bitrate;
    struct arg_end *end;
} device_can_bitrate_args;

static int print_device_summary(int argc, char **argv)
{  
    uint8_t can_id = 0;
    uint8_t can_type = 0;
    uint8_t hw_rev = 0;
    uint8_t can_bitrate = 0;
    char custom_string[9] {0};
    size_t custom_string_len = sizeof(custom_string);
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READONLY, &nvs_handle);
    nvs_get_u8(nvs_handle, "can_id", &can_id);
    nvs_get_u8(nvs_handle, "can_type", &can_type);
    nvs_get_u8(nvs_handle, "hw_rev", &hw_rev);
    nvs_get_u8(nvs_handle, "can_bitrate", &can_bitrate);
    nvs_get_str(nvs_handle, "custom_string", &custom_string[0], &custom_string_len);
    nvs_close(nvs_handle);
    
    std::string relais_remapping_str;
    
    printf("-------------------------------------\n");
    printf("Device Summary\n");

    const esp_app_desc_t* desc = esp_ota_get_app_description();
    printf("APP VERSION: %s\n", desc->version);
    printf("Device ID: %d\n", can_id);
    printf("Device Type: %d (%s)\n", can_type, ICAN::device_string(
        static_cast<ICAN::DEVICE_t>(can_type)));
    printf("Hardware Revision: %d\n", hw_rev);
    printf("Custom String: %s\n", custom_string);
    printf("CANBus bitrate: %s\n", ICAN::bitrate_string(
        static_cast<ICAN::BITRATE_t>(can_bitrate)));
    printf("-------------------------------------\n");   
    return 0;
}
static int set_device_id(int argc, char **argv)
{
    int nerrors = arg_parse(argc, argv, (void **) &device_rev_args);
    if (nerrors != 0) {
        arg_print_errors(stderr, device_rev_args.end, argv[0]);
        return 1;
    }
    assert(device_rev_args.rev->count == 1);
    int id = *device_rev_args.rev->ival;
    uint8_t id8 = static_cast<uint8_t>(id);
    ESP_LOGI(TAG, "Got device id %d", id);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_u8(nvs_handle, "can_id", id8);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    
    return 0;
}

static int set_device_type(int argc, char **argv)
{
    int nerrors = arg_parse(argc, argv, (void **) &device_type_args);
    if (nerrors != 0) {
        arg_print_errors(stderr, device_type_args.end, argv[0]);
        return 1;
    }
    assert(device_type_args.type->count == 1);
    std::string s(device_type_args.type->sval[0]);
    
    ICAN::DEVICE_t device = ICAN::device_type(s);
    if (device == ICAN::DEVICE_t::Unknown)
    {
        ESP_LOGE(TAG, "Device type %s in unknown", s.c_str());
        return 1;
    }
    ESP_LOGI(TAG, "Got device type %s", s.c_str());
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_u8(nvs_handle, "can_type", static_cast<uint8_t>(device));
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);

    return 0;
}

static int set_device_custom_string(int argc, char **argv)
{
    int nerrors = arg_parse(argc, argv, (void **) &device_custom_string_args);
    if (nerrors != 0) {
        arg_print_errors(stderr, device_custom_string_args.end, argv[0]);
        return 1;
    }
    assert(device_custom_string_args.custom_string->count == 1);
    std::string s(device_custom_string_args.custom_string->sval[0]);
    
    if (s.length() > 8)
    {
        ESP_LOGE(TAG, "Custom string must be max. 8 chars long");
        return 1;
    }
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "custom_string", s.c_str());
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);

    return 0;
}

static int set_device_rev(int argc, char **argv)
{
    int nerrors = arg_parse(argc, argv, (void **) &device_rev_args);
    if (nerrors != 0) {
        arg_print_errors(stderr, device_rev_args.end, argv[0]);
        return 1;
    }
    assert(device_rev_args.rev->count == 1);
    int rev = *device_rev_args.rev->ival;
    uint8_t rev8 = static_cast<uint8_t>(rev);
    ESP_LOGI(TAG, "Got device hardware revision %d", rev);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_u8(nvs_handle, "hw_rev", rev8);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    
    return 0;
}

static int set_device_can_bitrate(int argc, char **argv)
{
    int nerrors = arg_parse(argc, argv, (void **) &device_can_bitrate_args);
    if (nerrors != 0) {
        arg_print_errors(stderr, device_can_bitrate_args.end, argv[0]);
        return 1;
    }
    assert(device_can_bitrate_args.bitrate->count == 1);
    int bitrate = *device_can_bitrate_args.bitrate->ival;
    uint8_t bitrate8 = static_cast<uint8_t>(ICAN::bitrate(bitrate));
    ESP_LOGI(TAG, "Got device can bitrate %d", bitrate8);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_u8(nvs_handle, "can_bitrate", bitrate8);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    
    return 0;
}
static void register_summary(void)
{
    const esp_console_cmd_t cmd = {
        .command = "device_summary",
        .help = "Get a summary of device settings",
        .hint = NULL,
        .func = &print_device_summary,
        .argtable = NULL
    };
    ESP_ERROR_CHECK( esp_console_cmd_register(&cmd) );
}

static void register_id(void)
{   
    device_id_args.id = arg_int1(NULL, NULL, "<id>", "Device id");
    device_id_args.end = arg_end(1);
    const esp_console_cmd_t cmd = {
        .command = "device_id",
        .help = "Set the device id",
        .hint = NULL,
        .func = &set_device_id,
        .argtable = &device_id_args
    };
    ESP_ERROR_CHECK( esp_console_cmd_register(&cmd) );
}

static void register_type(void)
{   
    device_type_args.type = arg_str1(NULL, NULL, "<Button|Relais|Gateway|Rollershutter|SSR>", "Device type");
    device_type_args.end = arg_end(1);
    const esp_console_cmd_t cmd = {
        .command = "device_type",
        .help = "Set the device type",
        .hint = NULL,
        .func = &set_device_type,
        .argtable = &device_type_args
    };
    ESP_ERROR_CHECK( esp_console_cmd_register(&cmd) );
}

static void register_custom_string(void)
{   
    device_custom_string_args.custom_string = arg_str1(NULL, NULL, "<custom_string>", "Custom String");
    device_custom_string_args.end = arg_end(1);
    const esp_console_cmd_t cmd = {
        .command = "device_custom_string",
        .help = "Set the device custom string",
        .hint = NULL,
        .func = &set_device_custom_string,
        .argtable = &device_custom_string_args
    };
    ESP_ERROR_CHECK( esp_console_cmd_register(&cmd) );
}

static void register_rev(void)
{
    device_rev_args.rev = arg_int1(NULL, NULL, "<rev>", "Device hardware revision");
    device_rev_args.end = arg_end(1);
    const esp_console_cmd_t cmd = {
        .command = "device_rev",
        .help = "Set the device hardware revision",
        .hint = NULL,
        .func = &set_device_rev,
        .argtable = &device_rev_args
    };
    ESP_ERROR_CHECK( esp_console_cmd_register(&cmd) );
}

static void register_can_bitrate(void)
{   
    device_can_bitrate_args.bitrate = arg_int1(NULL, NULL, "<22222|25000|50000|100000>", "CANBus bitrate");
    device_can_bitrate_args.end = arg_end(1);
    const esp_console_cmd_t cmd = {
        .command = "device_can_bitrate",
        .help = "Set the device CABBus bitrate",
        .hint = NULL,
        .func = &set_device_can_bitrate,
        .argtable = &device_can_bitrate_args
    };
    ESP_ERROR_CHECK( esp_console_cmd_register(&cmd) );
}

ConsoleCommandDevice* ConsoleCommandDevice::console_command = nullptr;

ConsoleCommandDevice::ConsoleCommandDevice(Console& c)
{
    console_command = this;
}


void register_device()
{
    register_summary();
    register_id();
    register_type();
    register_custom_string();
    register_rev();
    register_can_bitrate();
}

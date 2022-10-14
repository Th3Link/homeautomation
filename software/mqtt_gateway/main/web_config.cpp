#include "web_config.h"
#include "update.h"

#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/semphr.h"
#include "esp_http_server.h"
#include "esp_chip_info.h"
#include "esp_vfs.h"
#include "esp_spiffs.h"
#include "esp_err.h"
#include "esp_log.h"
#include "cJSON.h"
#include <cstdio>
#include <fcntl.h>
#include "mbedtls/base64.h"

#include <algorithm>

static const char *TAG = "web_config";
#define SCRATCH_BUFSIZE (10240)

typedef struct {
    char base_path[ESP_VFS_PATH_MAX + 1];
    char scratch[SCRATCH_BUFSIZE];
} server_context_t;

#define FILE_PATH_MAX (ESP_VFS_PATH_MAX + 128)
#define CHECK_FILE_EXTENSION(filename, ext) (strcasecmp(&filename[strlen(filename) - strlen(ext)], ext) == 0)


static esp_err_t httpRequestAuthorization(httpd_req_t *req)
{
    httpd_resp_set_hdr(req, "WWW-Authenticate", "Basic realm=\"my_realm1\"");
    httpd_resp_set_status(req, "401 Unauthorized");
    httpd_resp_set_type(req, HTTPD_TYPE_TEXT);
    httpd_resp_sendstr(req, "Unauthorized");
    return ESP_OK;
}


static bool httpAuthenticateRequest(httpd_req_t *req, const char *server_username, const char *server_password)
{
    char  authorization_header[64] = {0};
    char decoded_authorization[32] = {0};
    size_t buf_len;

    // Get header value string length
    buf_len = httpd_req_get_hdr_value_len(req, "Authorization");

    //ESP_LOGD(TAG, "Authorization header length %d", buf_len);
    //bound check
    if ((buf_len > 0) && (buf_len < 64))
    {
        // Copy null terminated value string into buffer
        if (httpd_req_get_hdr_value_str(req, "Authorization", authorization_header, buf_len + 1) == ESP_OK)
        {
            //ESP_LOGD(TAG, "Authorization header : %s", authorization_header);
            
            //find the "Basic " part of the header
            char *encoded_authorization = strstr(authorization_header, "Basic ");
            if(encoded_authorization != NULL)
            {
                //move the pointer to the start of the encoded authorization string
                encoded_authorization = &encoded_authorization[strlen("Basic ")];

                //ESP_LOGD(TAG, "Authorization string : %s", encoded_authorization);

                //decode the authorization string
                int decode_res = mbedtls_base64_decode((unsigned char *)decoded_authorization, sizeof(decoded_authorization), &buf_len, (unsigned char *)encoded_authorization, strlen(encoded_authorization));
                if(decode_res == 0)
                {
                    //ESP_LOGD(TAG, "Decoded Authorization string : %s", decoded_authorization);

                    //find the separator between username:password
                    char *colon_index = strchr(decoded_authorization, ':');
                    if(colon_index != NULL)
                    {
                        //replace colon index with null termination 
                        colon_index[0] = 0;
                        //username is from start till our previous null termination
                        char *req_username = &decoded_authorization[0];
                        //the rest is the password
                        char *req_password = &colon_index[1];

                        //ESP_LOGD(TAG, "Username:%s, Password:%s", req_username, req_password);
                        
                        //check if both username and password match the server's credentials
                        if ((strcmp(req_username, server_username) == 0) && (strcmp(req_password, server_password) == 0))
                        {
                            return true;
                        }
                    }
                    else
                    {
                        //ESP_LOGD(TAG, "Decoede authorization does not contain password");
                    }
                }
                else
                {
                    //ESP_LOGD(TAG, "Decoding failed");
                }
            }
            else
            {
                //ESP_LOGD(TAG, "Authorization value not in correct format");
            }
        }
        else
        {
            //ESP_LOGD(TAG, "Cannot retrieve autorization value");
        }
    }
    else
    {
        //ESP_LOGD(TAG, "No autorization header or too long");
    }
    
    //ESP_LOGW(TAG, "Authentication Failed");
    return false;
}

static esp_err_t index_html_get_handler(httpd_req_t *req)
{
    if(httpAuthenticateRequest(req, "username", "password") == false)
    {
        return httpRequestAuthorization(req);
    }
    extern const uint8_t _index_html_start[] asm("_binary_index_html_start");
    extern const uint8_t _index_html_end[]   asm("_binary_index_html_end");
    const size_t _index_html_size = (_index_html_end - _index_html_start);
    httpd_resp_set_type(req, "text/html");
    httpd_resp_send(req, (const char *)_index_html_start, _index_html_size);
    return ESP_OK;
}

static esp_err_t favicon_png_get_handler(httpd_req_t *req)
{
    extern const uint8_t _favicon_png_start[] asm("_binary_favicon_png_start");
    extern const uint8_t _favicon_png_end[]   asm("_binary_favicon_png_end");
    const size_t _favicon_png_size = (_favicon_png_end - _favicon_png_start);
    httpd_resp_set_type(req, "image/png");
    httpd_resp_send(req, (const char *)_favicon_png_start, _favicon_png_size);
    return ESP_OK;
}

static esp_err_t minimal_js_get_handler(httpd_req_t *req)
{
    extern const uint8_t _minimal_js_start[] asm("_binary_minimal_js_start");
    extern const uint8_t _minimal_js_end[]   asm("_binary_minimal_js_end");
    const size_t _minimal_js_size = (_minimal_js_end - _minimal_js_start);
    httpd_resp_set_type(req, "text/javascript");
    httpd_resp_send(req, (const char *)_minimal_js_start, _minimal_js_size);
    return ESP_OK;
}

static esp_err_t style_css_get_handler(httpd_req_t *req)
{
    extern const uint8_t _style_css_start[] asm("_binary_style_css_start");
    extern const uint8_t _style_css_end[]   asm("_binary_style_css_end");
    const size_t _style_css_size = (_style_css_end - _style_css_start);
    httpd_resp_set_type(req, "text/css");
    httpd_resp_send(req, (const char *)_style_css_start, _style_css_size);
    return ESP_OK;
}

static esp_err_t config_json_get_handler(httpd_req_t *req)
{
    httpd_resp_set_type(req, "application/json");
    cJSON *root = cJSON_CreateObject();
    
    cJSON_AddBoolToObject(root, "factory_reset", false);
    cJSON_AddBoolToObject(root, "reboot", false);
    cJSON_AddStringToObject(root, "hostname", "HOSTNAME");
    cJSON *wifi = cJSON_AddObjectToObject(root, "wifi");
    cJSON_AddStringToObject(wifi, "mode", "AP");
    cJSON_AddStringToObject(wifi, "ssid", "SSID");
    cJSON_AddStringToObject(wifi, "password", "PASSWORD");
    cJSON *mqtt = cJSON_AddObjectToObject(root, "mqtt");
    cJSON_AddStringToObject(mqtt, "uri", "mqtt://IP:PORT");
    cJSON *canbus = cJSON_AddObjectToObject(root, "canbus");
    cJSON_AddStringToObject(canbus, "baudrate", "b100");
    
    const char *config_json = cJSON_Print(root);
    httpd_resp_sendstr(req, config_json);
    free((void *)config_json);
    cJSON_Delete(root);
    return ESP_OK;
}

static esp_err_t state_json_get_handler(httpd_req_t *req)
{    
    httpd_resp_set_type(req, "application/json");
    cJSON *root = cJSON_CreateObject();

    cJSON_AddStringToObject(root, "firmware_version", "0.0");
    cJSON_AddStringToObject(root, "uptime", "TIME");
    cJSON_AddStringToObject(root, "hostname", "HOSTNAME");
    cJSON *wifi = cJSON_AddObjectToObject(root, "wifi");
    cJSON_AddStringToObject(wifi, "state", "connected");
    cJSON_AddStringToObject(wifi, "ipv4", "IP");
    cJSON_AddStringToObject(wifi, "ipv6", "IP");
    cJSON_AddStringToObject(wifi, "gateway", "GATEWAY");
    cJSON_AddStringToObject(wifi, "dns", "DNS");
    cJSON *mqtt = cJSON_AddObjectToObject(root, "mqtt");
    cJSON_AddStringToObject(mqtt, "state", "connected");
    cJSON_AddNumberToObject(mqtt, "received", 130);
    cJSON_AddNumberToObject(mqtt, "sent", 100);
    cJSON *canbus = cJSON_AddObjectToObject(root, "canbus");
    cJSON_AddNumberToObject(canbus, "received", 10);
    cJSON_AddNumberToObject(canbus, "sent", 100);
    
    const char *state_json = cJSON_Print(root);
    httpd_resp_sendstr(req, state_json);
    free((void *)state_json);
    cJSON_Delete(root);
    return ESP_OK;
}

static esp_err_t device_list_json_get_handler(httpd_req_t *req)
{    
    httpd_resp_set_type(req, "application/json");
    cJSON *root = cJSON_CreateObject();

    const char *const header_arr[] = {"Unique ID", "Device ID", "Type", "Type Name", "Custom String","Last Seen (Minutes ago)", "State", "Error"};
    cJSON_AddArrayToObject(root, "warnings");
    cJSON* header = cJSON_CreateStringArray(header_arr, 8);
    cJSON_AddItemToObject(root, "header", header);
    cJSON *devices = cJSON_AddArrayToObject(root, "devices");
    
    cJSON* device = cJSON_CreateObject();
    cJSON_AddStringToObject(device, "uid", "0x123456781234567812345678");
    cJSON_AddStringToObject(device, "device_id", "0x02");
    cJSON_AddStringToObject(device, "device_type", "0x03");
    cJSON_AddStringToObject(device, "device_type_name", "LampController");
    cJSON_AddStringToObject(device, "custom_string", "ABCDEFLL");
    cJSON_AddNumberToObject(device, "last_seen", 345064);
    cJSON_AddStringToObject(device, "state", "application");
    cJSON_AddStringToObject(device, "last_error", "0x0");
    cJSON_AddItemToArray(devices, device);
    
    const char *state_json = cJSON_Print(root);
    httpd_resp_sendstr(req, state_json);
    free((void *)state_json);
    cJSON_Delete(root);
    return ESP_OK;
}

static esp_err_t update_device_state_post_handler(httpd_req_t *req)
{
    int total_len = req->content_len;
    int cur_len = 0;
    char *buf = ((server_context_t *)(req->user_ctx))->scratch;
    int received = 0;
    if (total_len >= SCRATCH_BUFSIZE) {
        /* Respond with 500 Internal Server Error */
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "content too long");
        return ESP_FAIL;
    }
    while (cur_len < total_len) {
        received = httpd_req_recv(req, buf + cur_len, total_len);
        if (received <= 0) {
            /* Respond with 500 Internal Server Error */
            httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "Failed to post control value");
            return ESP_FAIL;
        }
        cur_len += received;
    }
    buf[total_len] = '\0';

    cJSON *root = cJSON_Parse(buf);
    bool prepare = cJSON_IsTrue(cJSON_GetObjectItem(root, "prepare"));
    bool complete = cJSON_IsTrue(cJSON_GetObjectItem(root, "complete"));

    cJSON_Delete(root);
    httpd_resp_sendstr(req, "Post control value successfully");

    if (prepare)
    {
        update_start();
    } 
    else if (complete)
    {
        httpd_resp_set_hdr(req, "Connection", "close");
        httpd_resp_sendstr(req, "Update successfully");
        update_complete();
    }
    
    return ESP_OK;
}

bool header_complete(const char* input, const char* compare, size_t len)
{
    for (unsigned int i = 0; i < len; i++)
    {
        if (input[i] != compare[i])
        {
            return false;
        }
    }
    return true;
}

static esp_err_t update_device_data_post_handler(httpd_req_t *req)
{
    /* Retrieve the pointer to scratch buffer for temporary storage */
    char *header_buf = ((server_context_t *)req->user_ctx)->scratch;
    char *buf = ((server_context_t *)req->user_ctx)->scratch;
    const char* header_end = "\r\n\r\n";
    
    httpd_req_recv(req, header_buf, 4);
    header_buf += 4;

    while (!header_complete(header_buf-4, header_end, sizeof(header_end)))
    {
        httpd_req_recv(req, header_buf++, 1);
    }
    
    *header_buf = '\0';
    ESP_LOGI(TAG, "Header : %s", buf);
    
    int received;

    /* Content length of the request gives
     * the size of the file being uploaded */
    int remaining = req->content_len - (header_buf - buf);
    
    while (remaining > 0) {

        ESP_LOGI(TAG, "Remaining size : %d", remaining);
        /* Receive the file part by part into a buffer */
        if ((received = httpd_req_recv(req, buf, std::min(remaining, SCRATCH_BUFSIZE))) <= 0) {
            
            if (received == HTTPD_SOCK_ERR_TIMEOUT) {
                /* Retry if timeout occurred */
                continue;
            }

            /* In case of unrecoverable error,
             * close and delete the unfinished file*/
            update_abort();

            ESP_LOGE(TAG, "File reception failed!");
            /* Respond with 500 Internal Server Error */
            httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "Failed to receive file");
            return ESP_FAIL;
        }
        
        /* Write buffer content to file on storage */
        if (received && (!update_data(buf, received))) {
            update_abort();

            ESP_LOGE(TAG, "File write failed!");
            /* Respond with 500 Internal Server Error */
            httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "Failed to write file to storage");
            return ESP_FAIL;
        }

        /* Keep track of remaining size of
         * the file left to be uploaded */
        remaining -= received;
    }
    
    httpd_resp_sendstr(req, "File uploaded successfully");
    
    return ESP_OK;
}

void web_config_init()
{
    const char* base_path = "/web";
    
    esp_vfs_spiffs_conf_t conf = {
        .base_path = base_path,
        .partition_label = "web",
        .max_files = 5,
        .format_if_mount_failed = false
    };
    esp_err_t ret = esp_vfs_spiffs_register(&conf);

    if (ret != ESP_OK) {
        if (ret == ESP_FAIL) {
            ESP_LOGE(TAG, "Failed to mount or format filesystem");
        } else if (ret == ESP_ERR_NOT_FOUND) {
            ESP_LOGE(TAG, "Failed to find SPIFFS partition");
        } else {
            ESP_LOGE(TAG, "Failed to initialize SPIFFS (%s)", esp_err_to_name(ret));
        }
    }
    
    server_context_t* server_context = reinterpret_cast<server_context_t*>(
        calloc(1, sizeof(server_context_t)));
    strlcpy(server_context->base_path, base_path, sizeof(server_context->base_path));
    
    httpd_handle_t server = NULL;
    httpd_config_t config = HTTPD_DEFAULT_CONFIG();
    config.max_uri_handlers = 12;
    config.uri_match_fn = httpd_uri_match_wildcard;

    ESP_LOGI(TAG, "Starting HTTP Server");
    ESP_ERROR_CHECK(httpd_start(&server, &config));
    
    /* 
     * URI handler for device states (prepare and complete update) 
     * It may not be absolutely necessary but its more explicit and
     * easier to implement
     */
    httpd_uri_t update_device_state_post_uri = {
        .uri = "/update/device/state",
        .method = HTTP_POST,
        .handler = update_device_state_post_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &update_device_state_post_uri);
    
    /* URI handler to receive the update itself */
    httpd_uri_t update_device_data_post_uri = {
        .uri = "/update/device/data",
        .method = HTTP_POST,
        .handler = update_device_data_post_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &update_device_data_post_uri);
    
    httpd_uri_t index_html_get_uri = {
        .uri = "/index.html",
        .method = HTTP_GET,
        .handler = index_html_get_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &index_html_get_uri);

    httpd_uri_t root_get_uri = {
        .uri = "/",
        .method = HTTP_GET,
        .handler = index_html_get_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &root_get_uri);

    httpd_uri_t favicon_png_get_uri = {
        .uri = "/favicon.png",
        .method = HTTP_GET,
        .handler = favicon_png_get_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &favicon_png_get_uri);

    httpd_uri_t minimal_js_get_uri = {
        .uri = "/minimal.js",
        .method = HTTP_GET,
        .handler = minimal_js_get_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &minimal_js_get_uri);
    
    httpd_uri_t style_css_get_uri = {
        .uri = "/style.css",
        .method = HTTP_GET,
        .handler = style_css_get_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &style_css_get_uri);
    
    httpd_uri_t state_json_get_uri = {
        .uri = "/state.json",
        .method = HTTP_GET,
        .handler = state_json_get_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &state_json_get_uri);
    
    httpd_uri_t config_json_get_uri = {
        .uri = "/config.json",
        .method = HTTP_GET,
        .handler = config_json_get_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &config_json_get_uri);
    
    httpd_uri_t device_list_json_get_uri = {
        .uri = "/deviceList.json",
        .method = HTTP_GET,
        .handler = device_list_json_get_handler,
        .user_ctx = server_context
    };
    httpd_register_uri_handler(server, &device_list_json_get_uri);
}


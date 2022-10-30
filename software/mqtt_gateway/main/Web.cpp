#include "Web.hpp"
#include "http_handler.hpp"

#include <chrono>

#include <cJSON.h>
#include <esp_log.h>
#include <esp_err.h>
#include <esp_ota_ops.h>
#include <esp_timer.h>

const char* Web::TAG = "Web";

static esp_err_t update_data_post_handler(httpd_req_t *req)
{
    return reinterpret_cast<Web*>(req->user_ctx)->update_data_post_handler(req);
}

static esp_err_t state_json_get_handler(httpd_req_t *req)
{
    return reinterpret_cast<Web*>(req->user_ctx)->state_get_handler(req);
}

static esp_err_t control_json_post_handler(httpd_req_t *req)
{
    return reinterpret_cast<Web*>(req->user_ctx)->control_post_handler(req);
}

static bool header_complete(const char* input, const char* compare, size_t len)
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

Web::Web(Update& u, CANUpdate& cu, IMQTT& im, ICAN& ic, WiFi& w, Logging& l, DeviceList& d) : 
    m_command(u,cu,im,ic,w,l,m_web_credentials), m_mqtt(im), m_can(ic), m_wifi(w), m_logging(l), m_deviceList(d)
{

}

void Web::init()
{
    m_web_credentials.init();
    httpd_handle_t server = NULL;
    httpd_config_t config = HTTPD_DEFAULT_CONFIG();
    config.max_uri_handlers = 7;
    config.uri_match_fn = httpd_uri_match_wildcard;

    ESP_LOGI(TAG, "Starting HTTP Server");
    ESP_ERROR_CHECK(httpd_start(&server, &config));
    
    /* URI handler to receive the update itself */
    httpd_uri_t update_data_post_uri = {
        .uri = "/update/data",
        .method = HTTP_POST,
        .handler = ::update_data_post_handler,
        .user_ctx = this
    };
    httpd_register_uri_handler(server, &update_data_post_uri);
    
    httpd_uri_t root_get_uri = {
        .uri = "/",
        .method = HTTP_GET,
        .handler = http_handler::index_html_get_handler,
        .user_ctx = this
    };
    httpd_register_uri_handler(server, &root_get_uri);

    httpd_uri_t favicon_png_get_uri = {
        .uri = "/favicon.png",
        .method = HTTP_GET,
        .handler = http_handler::favicon_png_get_handler,
        .user_ctx = this
    };
    httpd_register_uri_handler(server, &favicon_png_get_uri);

    httpd_uri_t minimal_js_get_uri = {
        .uri = "/minimal.js",
        .method = HTTP_GET,
        .handler = http_handler::minimal_js_get_handler,
        .user_ctx = this
    };
    httpd_register_uri_handler(server, &minimal_js_get_uri);
    
    httpd_uri_t style_css_get_uri = {
        .uri = "/style.css",
        .method = HTTP_GET,
        .handler = http_handler::style_css_get_handler,
        .user_ctx = this
    };
    httpd_register_uri_handler(server, &style_css_get_uri);
    
    httpd_uri_t state_json_get_uri = {
        .uri = "/state.json",
        .method = HTTP_GET,
        .handler = ::state_json_get_handler,
        .user_ctx = this
    };
    httpd_register_uri_handler(server, &state_json_get_uri);
    
    httpd_uri_t control_json_post_uri = {
        .uri = "/control.json",
        .method = HTTP_POST,
        .handler = ::control_json_post_handler,
        .user_ctx = this
    };
    httpd_register_uri_handler(server, &control_json_post_uri);
}

esp_err_t Web::state_get_handler(httpd_req_t *req)
{
    httpd_resp_set_type(req, "application/json");
    cJSON *root = cJSON_CreateObject();
   
    cJSON_AddStringToObject(root, "hostname", m_wifi.hostname());
    cJSON_AddStringToObject(root, "username", username());
    cJSON_AddStringToObject(root, "password", password());
    cJSON *wifi = cJSON_AddObjectToObject(root, "wifi");
    cJSON_AddStringToObject(wifi, "mode", m_wifi.mode_str());
    cJSON_AddStringToObject(wifi, "ssid", m_wifi.ssid());
    cJSON_AddStringToObject(wifi, "password", m_wifi.password());
    cJSON *mqtt = cJSON_AddObjectToObject(root, "mqtt");
    cJSON_AddStringToObject(mqtt, "uri", m_mqtt.uri());
    cJSON_AddStringToObject(mqtt, "username", m_mqtt.username());
    cJSON_AddStringToObject(mqtt, "password", m_mqtt.password());
    cJSON *canbus = cJSON_AddObjectToObject(root, "canbus");
    cJSON_AddStringToObject(canbus, "baudrate", ICAN::bitrate_string(m_can.bitrate()));

    const esp_app_desc_t* desc = esp_ota_get_app_description();
    const char release_prefix[] = "release/";
    const size_t rpl = sizeof(release_prefix)-1;
    char version[10] {0};
    if (strncmp(desc->version, release_prefix, rpl) == 0)
    {
        strncpy(&version[0], &desc->version[rpl], 
            strlen(desc->version) - rpl);
    }
    else
    {
        strncpy(&version[0], desc->version, sizeof(version));
    }
    // assure /0 at the end
    version[9] = 0;
    uint32_t mins = std::chrono::duration_cast<std::chrono::minutes>(
        std::chrono::microseconds(esp_timer_get_time())).count();
    uint32_t days = mins / 60 / 24;
    uint32_t rem_mins = mins - (days * 60 * 24);
    
    char uptime[32];
    sprintf(&uptime[0], "%lu days, %lu minutes", days, rem_mins);
    
    cJSON_AddStringToObject(root, "firmware_version", version);
    cJSON_AddStringToObject(root, "idf_version", desc->idf_ver);
    cJSON_AddStringToObject(root, "uptime", &uptime[0]);
    cJSON_AddStringToObject(wifi, "state", "connected");
    cJSON_AddStringToObject(wifi, "ipv4", "IP");
    cJSON_AddStringToObject(wifi, "ipv6", "IP");
    cJSON_AddStringToObject(wifi, "gateway", "GATEWAY");
    cJSON_AddStringToObject(wifi, "dns", "DNS");
    cJSON_AddStringToObject(mqtt, "state", (m_mqtt.connected() ? "connected" : "not connected"));
    cJSON_AddNumberToObject(mqtt, "received", m_mqtt.received());
    cJSON_AddNumberToObject(mqtt, "sent", m_mqtt.transmitted());
    cJSON_AddNumberToObject(canbus, "received", m_can.received());
    cJSON_AddNumberToObject(canbus, "sent", m_can.transmitted());
    cJSON_AddBoolToObject(canbus, "mqtt_logging", m_logging.mqtt_logging());

    const char *const header_arr[] = {"Unique ID", "Device ID", "Type", "Type Name", "Custom String","Last Seen (Minutes ago)", "State", "Error"};
    cJSON_AddArrayToObject(root, "warnings");
    cJSON* header = cJSON_CreateStringArray(header_arr, 8);
    cJSON_AddItemToObject(root, "header", header);
    m_deviceList.output(root);
    
    const char *state_json = cJSON_Print(root);
    httpd_resp_sendstr(req, state_json);
    free((void *)state_json);
    cJSON_Delete(root);
    return ESP_OK;
}

esp_err_t Web::control_post_handler(httpd_req_t *req)
{
    if (http_handler::httpAuthenticateRequest(req, username(), password()) == false)
    {
        return http_handler::httpRequestAuthorization(req);
    }
    
    int total_len = req->content_len;
    int cur_len = 0;
    char *buf = scratch;
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

    cJSON* root = cJSON_Parse(buf);
    cJSON* com = cJSON_GetObjectItem(root, "command");
    if (cJSON_IsString(com))
    {
        char* command = cJSON_GetStringValue(com);
        m_command.command(command, root);
    }
    
    cJSON_Delete(root);
    httpd_resp_sendstr(req, "Post control value successfully");
    return ESP_OK;
}

esp_err_t Web::update_data_post_handler(httpd_req_t *req)
{
    char *header_buf = scratch;
    char *buf = scratch;
    const char* header_end = "\r\n\r\n";
    
    if (m_command.current_update() == nullptr)
    {
        ESP_LOGE(TAG, "Update not prepared!");
        /* Respond with 500 Internal Server Error */
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "Update not prepared");
        return ESP_FAIL;
    }
    
    httpd_req_recv(req, header_buf, 4);
    header_buf += 4;

    while (!header_complete(header_buf-4, header_end, sizeof(header_end)))
    {
        httpd_req_recv(req, header_buf++, 1);
    }
    
    *header_buf = '\0';
    ESP_LOGI(TAG, "Header : %s", buf);
    
    size_t received;

    /* Content length of the request gives
     * the size of the file being uploaded */
    size_t remaining = req->content_len - (header_buf - buf);
    
    constexpr size_t RECEIVE_MAX = 400;
    
    while (remaining > 0) {

        ESP_LOGI(TAG, "Remaining size : %d", remaining);
        /* Receive the file part by part into a buffer */
        if ((received = httpd_req_recv(req, buf, std::min(remaining, RECEIVE_MAX))) <= 0) {
            
            if (received == HTTPD_SOCK_ERR_TIMEOUT) {
                /* Retry if timeout occurred */
                continue;
            }

            /* In case of unrecoverable error,
             * close and delete the unfinished file*/
            m_command.current_update()->abort();
            ESP_LOGE(TAG, "File reception failed!");
            /* Respond with 500 Internal Server Error */
            httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "Failed to receive file");
            return ESP_FAIL;
        }
        
        bool data_success = false;
        /* Write buffer content to file on storage */
        data_success = m_command.current_update()->data(buf, received);
        if (received && !data_success) {
            
            m_command.current_update()->abort();
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

const char* Web::username()
{
    return m_web_credentials.username();
}

const char* Web::password()
{
    return m_web_credentials.password();
}

#include "http_handler.hpp"
#include "mbedtls/base64.h"

esp_err_t httpRequestAuthorization(httpd_req_t *req)
{
    httpd_resp_set_hdr(req, "WWW-Authenticate", "Basic realm=\"my_realm1\"");
    httpd_resp_set_status(req, "401 Unauthorized");
    httpd_resp_set_type(req, HTTPD_TYPE_TEXT);
    httpd_resp_sendstr(req, "Unauthorized");
    return ESP_OK;
}


bool httpAuthenticateRequest(httpd_req_t *req, const char *server_username, const char *server_password)
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

esp_err_t index_html_get_handler(httpd_req_t *req)
{
    if (httpAuthenticateRequest(req, "username", "password") == false)
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

esp_err_t favicon_png_get_handler(httpd_req_t *req)
{
    extern const uint8_t _favicon_png_start[] asm("_binary_favicon_png_start");
    extern const uint8_t _favicon_png_end[]   asm("_binary_favicon_png_end");
    const size_t _favicon_png_size = (_favicon_png_end - _favicon_png_start);
    httpd_resp_set_type(req, "image/png");
    httpd_resp_send(req, (const char *)_favicon_png_start, _favicon_png_size);
    return ESP_OK;
}

esp_err_t minimal_js_get_handler(httpd_req_t *req)
{
    extern const uint8_t _minimal_js_start[] asm("_binary_minimal_js_start");
    extern const uint8_t _minimal_js_end[]   asm("_binary_minimal_js_end");
    const size_t _minimal_js_size = (_minimal_js_end - _minimal_js_start);
    httpd_resp_set_type(req, "application/javascript");
    httpd_resp_send(req, (const char *)_minimal_js_start, _minimal_js_size);
    return ESP_OK;
}

esp_err_t style_css_get_handler(httpd_req_t *req)
{
    extern const uint8_t _style_css_start[] asm("_binary_style_css_start");
    extern const uint8_t _style_css_end[]   asm("_binary_style_css_end");
    const size_t _style_css_size = (_style_css_end - _style_css_start);
    httpd_resp_set_type(req, "text/css");
    httpd_resp_send(req, (const char *)_style_css_start, _style_css_size);
    return ESP_OK;
}

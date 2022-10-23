#include "esp_http_server.h"
#include "esp_err.h"

namespace http_handler
{
    esp_err_t httpRequestAuthorization(httpd_req_t *req);
    bool httpAuthenticateRequest(httpd_req_t *req, const char *server_username, 
        const char *server_password);
    esp_err_t index_html_get_handler(httpd_req_t *req);
    esp_err_t favicon_png_get_handler(httpd_req_t *req);
    esp_err_t minimal_js_get_handler(httpd_req_t *req);
    esp_err_t style_css_get_handler(httpd_req_t *req);
}

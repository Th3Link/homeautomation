#ifndef __WIFI_H__
#define __WIFI_H__

#ifdef __cplusplus
extern "C" {
#endif

#include <stdbool.h>

void wifi_init_softap(const char* ssid, const char* password, const unsigned char channel);
bool wifi_init(const char* ssid, const char* password);

#ifdef __cplusplus
}
#endif


#endif //__WIFI_H__


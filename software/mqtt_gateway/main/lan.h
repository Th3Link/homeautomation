#ifndef __LAN_H__
#define __LAN_H__

#ifdef __cplusplus
extern "C" {
#endif

#include "freertos/semphr.h"

void lan_init(SemaphoreHandle_t* sem);

#ifdef __cplusplus
}
#endif


#endif //__LAN_H__




#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void update_start();
void update_abort();
bool update_data(char* data, uint32_t data_len);
void update_complete();
void update_verified();

#ifdef __cplusplus
}
#endif

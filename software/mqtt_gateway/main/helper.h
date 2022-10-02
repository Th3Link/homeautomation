#ifndef __HELPER_H__
#define __HELPER_H__

#ifdef __cplusplus
extern "C" {
#endif    
    void requestSubscribe();
    void mqtt_dispatch(const char* topic, unsigned int topic_len, const char* data, unsigned int data_len);
    void can_dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len);
    
    //implmented in main.c
    void mqtt_publish(const char* topic, const char* data);
    void can_send(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request);
    void mqtt_subscribe(const char* topic);
    
#ifdef __cplusplus
}
#endif

#endif //__HELPER_H__

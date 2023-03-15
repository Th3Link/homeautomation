import argparse
import os
import time
import sys
import paho.mqtt.client as mqtt

def on_message(client, userdata, message):
    print("on_message")
    print("message received " ,str(message.payload.decode("utf-8")))
    print("message topic=",message.topic)
    print("message qos=",message.qos)
    print("message retain flag=",message.retain)

def on_canbus_log(client, userdata, message):
    print("on_canbus_log")
    print("message received " ,str(message.payload.decode("utf-8")))
    print("message topic=",message.topic)
    print("message qos=",message.qos)
    print("message retain flag=",message.retain)
    with open('upload_log.txt', 'a') as f:
        f.write(str(message.payload.decode("utf-8"))[2:]+"\n")

def main():
    """
    parser = argparse.ArgumentParser(description='Update CAN Firmware')
    parser.add_argument('can_id', metavar='CAN_ID', type=auto_int,
            help='CAN to for the device to be updated')
    parser.add_argument('-y', '--yes', action='store_true', default=False, 
            help='do not ask if unsure, just to it (for automation)')
    parser.add_argument('-i', '--interface', metavar='INTERFACE', type=str, default='can0',
            help='name of the can interface (socketcan)')
    parser.add_argument('-b', '--bitrate', metavar='BITRATE', type=int, default=50000,
            help='bit rate for the can interface)')
    parser.add_argument('-u', '--upload_file', metavar='UPDATE_FILE', type=str,
            help='update file (*.bin)')
    parser.add_argument('-d', '--download_file', metavar='DOWNLOAD_FILE', type=str,
            help='readback file (*.bin)')
    parser.add_argument('-r', '--rescue_run', action='store_true', default=False, 
            help='do a rescue run because the firmware does not run anymore. Start this script first an then power cycle the device')
    parser.add_argument('--device_id', metavar='DEVICE_ID', type=auto_int, 
        help='set the device id. must set together with --device_type')
    parser.add_argument('--device_type', metavar='DEVICE_TYPE', type=auto_int,
        help='set the device type. must set together with --device_id')
    parser.add_argument('--groups', metavar='GROUPS', nargs='+', type=auto_int,
        help='programm groups into the device')
    parser.add_argument('--description_file', metavar='DESCRIPTION_FILE', type=auto_int,
        help='append the device id/type changes to this file')
    args = parser.parse_args()
    """
    client = mqtt.Client("can_log_reader")
    client.connect("192.168.178.54")
    client.publish("test", payload="test")
    client.subscribe("canbus/log/#")
    client.on_message = on_message
    client.message_callback_add("canbus/log/0x10040713", on_canbus_log)
    #client.message_callback_add("canbus/log/0x1004071e", on_canbus_log)
    client.loop_forever()
    
if __name__ == "__main__":
    # execute only if run as a script
    main()

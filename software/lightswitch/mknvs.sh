#!/bin/bash
for i in {1..9}
do
   ../esp-idf/components/nvs_flash/nvs_partition_generator/nvs_partition_gen.py generate nvs_$i.csv nvs_$i.bin 12288
done

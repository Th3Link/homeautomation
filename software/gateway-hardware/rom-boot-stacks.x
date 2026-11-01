/* The ROM's boot stacks in SRAM1, for statics that need no initial value
   (see docs/ram.md in gateway-core).

   esp-hal's memory.x keeps 0x3FFE0000-0x3FFE7E30 out of every region it
   places anything in. ESP-IDF keeps only the ROM's data there
   (components/heap/port/esp32/memory_layout.c: 0x3FFE0000-0x3FFE0440 and
   0x3FFE3F20-0x3FFE4350) and gives the rest to its heap once it runs; so
   do we, as two sections of our own. Like `.dram2_uninit` they are not
   zeroed and not loaded: whatever is placed there is initialised at run
   time. */
MEMORY
{
  /* The PRO CPU's boot stack: the ROM and the bootloader run on it, the
     firmware's entry code switches to its own stack before anything else. */
  rom_boot_pro_seg (RW) : ORIGIN = 0x3FFE0440, LENGTH = 0x3FFE3F20 - 0x3FFE0440
  /* The APP CPU's boot stack. Only free while the APP CPU is never started:
     the ROM runs on it when it is. */
  rom_boot_app_seg (RW) : ORIGIN = 0x3FFE4350, LENGTH = 0x3FFE7E30 - 0x3FFE4350
}

SECTIONS
{
  .rom_boot_pro_uninit (NOLOAD) : ALIGN(4) {
    *(.rom_boot_pro_uninit .rom_boot_pro_uninit.*)
  } > rom_boot_pro_seg

  .rom_boot_app_uninit (NOLOAD) : ALIGN(4) {
    *(.rom_boot_app_uninit .rom_boot_app_uninit.*)
  } > rom_boot_app_seg
}

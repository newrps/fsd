/* STM32H753ZI 메모리 맵.
 *
 * Flash:   2 MB at 0x0800_0000
 * DTCM:    128 KB at 0x2000_0000   (stack/critical data 권장)
 * AXI SRAM:512 KB at 0x2400_0000   (heap/buffer 권장)
 * SRAM1:   128 KB at 0x3000_0000
 * SRAM2:   128 KB at 0x3002_0000
 * SRAM3:   32 KB  at 0x3004_0000
 * SRAM4:   64 KB  at 0x3800_0000   (D3 도메인)
 * Backup SRAM: 4 KB at 0x3880_0000
 *
 * embassy-stm32 의 `memory-x` feature 가 활성화되면 default linker script 가
 * 제공되지만, NUCLEO-H753ZI 의 정확한 영역을 명시적으로 지정해 둔다.
 */
MEMORY
{
    FLASH  : ORIGIN = 0x08000000, LENGTH = 2M
    RAM    : ORIGIN = 0x24000000, LENGTH = 512K   /* AXI SRAM, code 실행 가능 */
}

/* cortex-m-rt 가 사용하는 심볼들 */
_stack_start = ORIGIN(RAM) + LENGTH(RAM);

/*
  HID Keyboard mouse combo example, compatible with QMK via protoco for remapping


  created 2023
  by Deqing Sun for use with CH55xduino

  This is a keyboard firmware that you can use usevia.app to remap the keys.

  The keyboard remap protocol is based on the QMK firmware
  The via impelementation is based on the CH552duinoKeyboard from yswallow

  In this example, the CH552 works as 3 key keyboard without matrix scanning.
  There are 2 layers. If the host system is linux or windows, the first layer is used.
  If the host system is macos or ios, the second layer is used.
  When the dataflash is empty, the default keymap is used. Which is copy, paste and tab.

  The remap can be done on the via website directly:
  https://usevia.app/
  Or remap-keys with the json file in the same folder:
  https://remap-keys.app/

  The circuit design, case design and photos can be found in pcb/keyboard

  cli board options: usb_settings=user266

*/

//For windows user, if you ever played with other HID device with the same PID C55D
//You may need to uninstall the previous driver completely
#include <WS2812.h>

#define NUM_LEDS 1
#define COLOR_PER_LEDS 3
#define NUM_BYTES (NUM_LEDS*COLOR_PER_LEDS)
__xdata uint8_t ledData[NUM_BYTES];
#ifndef USER_USB_RAM
#error "This example needs to be compiled with a USER USB setting"
#endif

#include "src/userQmkCompatibleKeyboard/USBHIDKeyboardMouse.h"
#include "keyboardConfig.h"

//these variables will be externally refered by the via library, they shall match the json file
//on ch552 there is 128 Byte of data flash, so we keep the row*col*layer to be less than 64 (2Byte each key)

#define BUTTON1_PIN 15
#define LED_BUILTIN 14

bool button1PressPrev = false;

unsigned long previousHelloMillis = 0;        // will store last time LED was updated
unsigned long previousKeyScanMillis = 0;

uint8_t layerInUse = 0;

volatile int counter = 0;
int max = 1000;
void counterManager(){
  counter++;
  if(counter>max)counter = max;
}

float calcValue(float x){
  float y=0;
  //定数設定
  float a = 0.01;//傾き
  float b = 1.05;//雪片
  float offset=0.2;//低照度時の明るさ

  y = -1 * a * x + b;//計算式(一次式)

  //limit処理
  if(y<offset)y = offset;
  if(y>1)y=1;

  return y;
}

int red =0;
int green=0;
int blue =0;

float floor(float x) {
    if (x >= 0.0f) {
        return (float)((int)x);
    } else {
        float t = (float)((int)x);
        return (t == x) ? t : t - 1.0f;
    }
}

void HSVtoRGB(float hue, float saturation, float value) {
    int i;
    float f, p, q, t;

    if (saturation == 0) {
        // Achromatic (gray)
        red = green = blue = value * 255;
        return;
    }

    hue /= 60;
    i = floor(hue);
    f = hue - i; // factorial part of hue
    p = value * (1 - saturation);
    q = value * (1 - saturation * f);
    t = value * (1 - saturation * (1 - f));

    switch (i) {
        case 0:
            red = value * 255;
            green = t * 255;
            blue = p * 255;
            break;
        case 1:
            red = q * 255;
            green = value * 255;
            blue = p * 255;
            break;
        case 2:
            red = p * 255;
            green = value * 255;
            blue = t * 255;
            break;
        case 3:
            red = p * 255;
            green = q * 255;
            blue = value * 255;
            break;
        case 4:
            red = t * 255;
            green = p * 255;
            blue = value * 255;
            break;
        default: // case 5:
            red = value * 255;
            green = p * 255;
            blue = q * 255;
            break;
    }
}

//タイマー呼び出し関数
void Timer2Interrupt(void) __interrupt (INT_NO_TMR2)
{
  TF2 = 0; // Timer2割り込みフラグをクリア
  counterManager();//counter
//  HSVtoRGB(100,1,calcValue(counter));//黄色
//  HSVtoRGB(140,1,calcValue(counter));//ショッキングピンク
//  HSVtoRGB(140,0.7,calcValue(counter));//ピンク
//  HSVtoRGB(200,1,calcValue(counter));//紫
//  HSVtoRGB(250,1,calcValue(counter));//青
//  HSVtoRGB(300,1,calcValue(counter));//ライトブルー
//  HSVtoRGB(350,1,calcValue(counter));//エメラルドグリーン
//  HSVtoRGB(0,1,calcValue(counter));//緑
//  HSVtoRGB(50,1,calcValue(counter));//ライムイエロー
//  HSVtoRGB(125,1,calcValue(counter));//赤
  HSVtoRGB(110,1,calcValue(counter));//オレンジ
 // HSVtoRGB(0,0,calcValue(counter));//白
  set_pixel_for_GRB_LED(ledData,0, green,red,blue);
  neopixel_show_P1_7(ledData, NUM_BYTES);

  //LED点灯
  digitalWrite(LED_BUILTIN, HIGH); 
  
   
}
void eeprom_init(){

    __data uint8_t allConfigFF = 1;
    __data uint8_t dataLength = ROWS_COUNT * COLS_COUNT * LAYER_COUNT * 2;
    //keyマップを確認
    for (__data uint8_t i = 0; i < dataLength; i++) {
      if (eeprom_read_byte(i) != 0xFF) {
        allConfigFF = 0;
        break;
      }
    }
    //eepromにconfig値がない場合は初期値を書き込み
    if (allConfigFF) {
      //write the default keymap
      const uint8_t defaultKeymap[] = {KEY_NONE, KEY_TAB};
      for (__data uint8_t i = 0; i < dataLength; i++) {
        eeprom_write_byte(i, defaultKeymap[i]);
      }
    }
  
}
void pin_init(){ //initialize the keys or matrix.
    pinMode(BUTTON1_PIN, INPUT_PULLUP);
    pinMode(LED_BUILTIN, OUTPUT);
    pinMode(17, OUTPUT); //Possible to use other pins. 
}
void timer_init(){
  //タイマ割り込み
  TR2 = 0; // Timer2をストップ（初期設定のため）
  C_T2 = 0; // Timer2クロックを内蔵クロック（デフォルト値のまま）
  T2MOD |= bTMR_CLK | bT2_CLK; //Timer2のクロックにFsys(24MHz)を使う
  //RCAP2H = 0xff; RCAP2L = 0x91; // Timer2オーバーフロー時のリロード値(0xff91=65425)
  RCAP2H = 0x00; RCAP2L = 0x00; // Timer2オーバーフロー時のリロード値(0xff91=65425)
  // Timer2は(65536-65425=111クロックでオーバーフローする。
  // つまりTimer2割り込みの周波数は24MHz/111=216kHz
  TH2 = RCAP2H; TL2 = RCAP2L; // Timer2のカウント値をリロード値（RCAP2）で初期化
  TF2 = 0; // Timer2割り込みフラグをクリア
  ET2 = 1;  //Timer2割り込みを許可
  EA = 1;  // グローバル割り込みを許可
  TR2 = 1;  // Timer2をスタート
}

void check_btn(){
    if ((signed int)(millis() - previousKeyScanMillis) >= 50) { //naive debouncing
    // scan the keys or matrix.
    previousKeyScanMillis = millis();


    //mac or win で レイヤー切り替え
    __data uint8_t osDetected = detected_host_os();
    if ((osDetected == OS_LINUX) || (osDetected == OS_WINDOWS)) {
      layerInUse = 0;
    } else if ((osDetected == OS_MACOS) || (osDetected == OS_IOS)) {
      layerInUse = 0;
    }
    
    bool button1Press = !digitalRead(BUTTON1_PIN);
    if(button1Press)counter = 0;                      

    if (button1PressPrev != button1Press) {
      button1PressPrev = button1Press;
      
      //QMK仕様で動作させる
      press_qmk_key(0, 0, layerInUse, button1Press);
      
      //任意の文字列を書き込む
      //Keyboard_write('H');
      //Keyboard_write('e');
      //Keyboard_write('l');
      //Keyboard_write('l');
      //Keyboard_write('o');
    }
  }
}

void setup() {
  USBInit();
  eeprom_init();
  pin_init();
  timer_init();
}

void loop() {

  via_process();
  check_btn();

}

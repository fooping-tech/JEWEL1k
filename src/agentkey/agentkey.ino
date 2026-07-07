/*
  JEWEL1k agent-key firmware (CH552E / CH55xduino)

  1key.ino をベースに、AIコーディングエージェント用の
  「ステータスLED + 物理承認ボタン」プロトコルへ拡張したものです。
  (created 2023 by Deqing Sun for use with CH55xduino をベースに改変)

  このバリアントは USB CDC シリアルデバイスとして振る舞います。
  QMK/via の HID キーボード機能はこのファームでは無効です
  (キーボードと併用したい場合は 1key.ino を書き込み直してください。
   HID 複合デバイス化は HidTransport 対応時に予定)。

  プロトコル (docs/PROTOCOL.md 参照):
    PC -> JEWEL1k : A1 state risk pattern brightness checksum (6 bytes)
    JEWEL1k -> PC : B1 event checksum                         (3 bytes)
    checksum = 先行する全バイトの XOR

  ボタンジェスチャ (意味づけはホスト側 ApprovalQueue が行う):
    単押し(<800ms)                 -> 0x01 Single   (承認には使われない)
    ダブル押し(350ms以内に2回)      -> 0x02 Double   (承認)
    長押し(>=800ms)                -> 0x03 Long     (拒否)
    超長押し(>=3000ms, 押下中に送信) -> 0x04 VeryLong (緊急停止)
    生イベント Down/Up             -> 0x05 / 0x06

  Arduino IDE ボード設定: CH552, USB Settings は "Default CDC" のまま
  (1key.ino と違い usb_settings=user266 は不要)
*/

#include <WS2812.h>

#define NUM_LEDS 1
#define COLOR_PER_LEDS 3
#define NUM_BYTES (NUM_LEDS * COLOR_PER_LEDS)
__xdata uint8_t ledData[NUM_BYTES];

#define BUTTON1_PIN 15
#define LED_BUILTIN 14

// ---- protocol constants ----------------------------------------------
#define HOST_HEADER 0xA1
#define DEV_HEADER 0xB1

#define EV_SINGLE 0x01
#define EV_DOUBLE 0x02
#define EV_LONG 0x03
#define EV_VERYLONG 0x04
#define EV_DOWN 0x05
#define EV_UP 0x06
#define EV_READY 0x10

#define ST_IDLE 0
#define ST_THINKING 1
#define ST_TOOL 2
#define ST_DONE 3
#define ST_APPROVAL 4
#define ST_ERROR 5
#define ST_OFF 6

#define PAT_OFF 0
#define PAT_SOLID 1
#define PAT_BREATH 2
#define PAT_BLINK 3
#define PAT_DOUBLE 4
#define PAT_FAST 5

// ---- gesture timing ----------------------------------------------------
#define DEBOUNCE_MS 30
#define LONG_MS 800
#define VERYLONG_MS 3000
#define DOUBLE_WINDOW_MS 350

// ---- current LED command (host が更新) ---------------------------------
uint8_t curState = ST_IDLE;
uint8_t curRisk = 0;
uint8_t curPattern = PAT_BREATH;
uint8_t curBrightness = 40;

// ---- receive state machine ---------------------------------------------
__xdata uint8_t rxBuf[6];
uint8_t rxLen = 0;

// ---- button state --------------------------------------------------------
bool btnPrev = false;
unsigned long btnChangeMillis = 0;
unsigned long pressStartMillis = 0;
bool veryLongSent = false;
// 単押し確定待ち (ダブル判定ウィンドウ)
bool clickPending = false;
unsigned long clickPendingMillis = 0;

unsigned long lastRenderMillis = 0;

// ---- device -> host -------------------------------------------------------
void sendEvent(uint8_t ev) {
  USBSerial_write(DEV_HEADER);
  USBSerial_write(ev);
  USBSerial_write(DEV_HEADER ^ ev);
  USBSerial_flush();
}

// ---- host -> device ---------------------------------------------------------
void pollSerial() {
  while (USBSerial_available()) {
    uint8_t b = USBSerial_read();
    if (rxLen == 0 && b != HOST_HEADER) {
      continue; // 再同期: ヘッダ待ち
    }
    rxBuf[rxLen++] = b;
    if (rxLen >= 6) {
      rxLen = 0;
      uint8_t ck = rxBuf[0] ^ rxBuf[1] ^ rxBuf[2] ^ rxBuf[3] ^ rxBuf[4];
      if (ck != rxBuf[5]) continue; // 破損フレームは無視
      curState = rxBuf[1];
      curRisk = rxBuf[2];
      curPattern = rxBuf[3];
      curBrightness = rxBuf[4];
    }
  }
}

// ---- LED ------------------------------------------------------------------
// state -> 色 (agent-key-core led_policy::color_for と同じテーブル)
void stateColor(uint8_t state, uint8_t *r, uint8_t *g, uint8_t *b) {
  switch (state) {
    case ST_THINKING: *r = 0;   *g = 60;  *b = 255; break; // 青
    case ST_TOOL:     *r = 255; *g = 180; *b = 0;   break; // 黄
    case ST_DONE:     *r = 0;   *g = 255; *b = 60;  break; // 緑
    case ST_APPROVAL: *r = 255; *g = 0;   *b = 0;   break; // 赤
    case ST_ERROR:    *r = 255; *g = 0;   *b = 30;  break; // 赤
    case ST_OFF:      *r = 0;   *g = 0;   *b = 0;   break;
    default:          *r = 40;  *g = 40;  *b = 40;  break; // idle: 薄い白
  }
}

// pattern -> 明るさエンベロープ 0..255 (整数のみ、浮動小数は使わない)
uint8_t patternEnvelope(uint8_t pattern, unsigned long now) {
  uint16_t t;
  switch (pattern) {
    case PAT_OFF:
      return 0;
    case PAT_SOLID:
      return 255;
    case PAT_BREATH:
      // 2.4s 周期の三角波。消灯しきらない (min 40) ので「柔らかく点滅」
      t = now % 2400;
      if (t >= 1200) t = 2400 - t;           // 0..1200 の三角波
      return 40 + (uint8_t)((uint32_t)t * 215 / 1200);
    case PAT_BLINK:
      return (now % 1000) < 500 ? 255 : 0;
    case PAT_DOUBLE:
      // ダブル点滅: 120ms x2 のフラッシュ + 休止 (周期 1.2s)
      t = now % 1200;
      if (t < 120) return 255;
      if (t < 240) return 0;
      if (t < 360) return 255;
      return 0;
    case PAT_FAST:
      return (now % 200) < 100 ? 255 : 0;
    default:
      return 255;
  }
}

void renderLed(unsigned long now) {
  uint8_t r, g, b;
  stateColor(curState, &r, &g, &b);
  uint16_t env = patternEnvelope(curPattern, now);
  // 色 * エンベロープ * マスタ輝度
  r = (uint8_t)(((uint16_t)r * env / 255) * curBrightness / 255);
  g = (uint8_t)(((uint16_t)g * env / 255) * curBrightness / 255);
  b = (uint8_t)(((uint16_t)b * env / 255) * curBrightness / 255);
  // Helper accepts RGB arguments and stores them in GRB byte order.
  set_pixel_for_GRB_LED(ledData, 0, r, g, b);
  neopixel_show_P1_7(ledData, NUM_BYTES);
}

// ---- button ----------------------------------------------------------------
void checkButton(unsigned long now) {
  // 単押し確定: ダブル判定ウィンドウが閉じた
  if (clickPending && (now - clickPendingMillis) >= DOUBLE_WINDOW_MS) {
    clickPending = false;
    sendEvent(EV_SINGLE);
  }

  if ((now - btnChangeMillis) < DEBOUNCE_MS) return; // naive debouncing

  bool pressed = !digitalRead(BUTTON1_PIN);

  // 押下中に超長押し閾値を跨いだら即時に緊急停止イベント
  if (pressed && btnPrev && !veryLongSent && (now - pressStartMillis) >= VERYLONG_MS) {
    veryLongSent = true;
    sendEvent(EV_VERYLONG);
  }

  if (pressed == btnPrev) return;
  btnPrev = pressed;
  btnChangeMillis = now;

  if (pressed) {
    pressStartMillis = now;
    veryLongSent = false;
    sendEvent(EV_DOWN);
  } else {
    sendEvent(EV_UP);
    unsigned long dur = now - pressStartMillis;
    if (veryLongSent) {
      // 超長押しは押下中に送信済み
    } else if (dur >= LONG_MS) {
      sendEvent(EV_LONG);
    } else if (clickPending) {
      // ウィンドウ内の2回目の短押し -> ダブル
      clickPending = false;
      sendEvent(EV_DOUBLE);
    } else {
      clickPending = true;
      clickPendingMillis = now;
    }
  }
}

// ---- arduino entry points -----------------------------------------------
void setup() {
  pinMode(BUTTON1_PIN, INPUT_PULLUP);
  pinMode(LED_BUILTIN, OUTPUT);
  digitalWrite(LED_BUILTIN, HIGH);
  // ホスト側が接続を検出できるように起床通知
  delay(100);
  sendEvent(EV_READY);
}

void loop() {
  unsigned long now = millis();
  pollSerial();
  checkButton(now);
  if ((now - lastRenderMillis) >= 16) { // ~60fps
    lastRenderMillis = now;
    renderLed(now);
  }
}

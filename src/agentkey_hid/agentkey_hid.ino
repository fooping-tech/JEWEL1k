/*
  JEWEL1k agent-key 複合デバイスファームウェア (CH552E / CH55xduino)

  Phase 5: キーボードHID + vendor raw HID (usage page 0xFF60, usage 0x61) の
  複合デバイス。1key.ino の QMK/via 互換キーボード機能をそのまま残しつつ、
  agent-key プロトコル (docs/PROTOCOL.md) を raw HID レポートに載せる:

    PC -> JEWEL1k : 32byteレポート先頭に A1 state risk pattern brightness checksum
    JEWEL1k -> PC : 32byteレポート先頭に B1 event checksum (残りは0埋め)

  raw HID インターフェースは via (usevia.app) と共用。0xA1 は via コマンドID
  (0x01..0x12, 0xFF) と衝突しない。ボタンイベント(B1)は非同期に IN レポートで
  送るため、via クライアントの使用中はレポートが混在しうる点に注意。

  ボタン:
    - 通常時 (curState != ST_APPROVAL): via キーマップ通りのキー入力
      (Down/Up で press_qmk_key、単押しは即時にHIDキーとして有効)。
      誤入力を避けるため、デフォルトキーマップは KEY_NONE (何も入力しない)。
      キーとして使いたい場合は usevia.app で任意のキーに割り当てる
    - 承認待ち中 (curState == ST_APPROVAL): HIDキー入力を抑止する
      (押下時に key down を送らない。承認クリックがホストへ文字入力として
      漏れないようにするため)。B1 イベント送信は通常時と同様に行う
    - agent-key ジェスチャを分類して B1 イベント送信 (状態によらず常に):
        単押し(<800ms)                 -> 0x01 Single   (承認には使われない)
        ダブル押し(350ms以内に2回)      -> 0x02 Double   (承認)
        長押し(>=800ms)                -> 0x03 Long     (拒否)
        超長押し(>=3000ms, 押下中に送信) -> 0x04 VeryLong (緊急停止)
        生イベント Down/Up             -> 0x05 / 0x06

  LED レンダリング・ジェスチャ閾値は src/agentkey/agentkey.ino (CDC版) と同一。

  Arduino IDE ボード設定: CH552, USB Settings = "USER CODE /w 266B ram"
  (1key.ino と同じ。cli board options: usb_settings=user266)

  ※ この環境ではコンパイルできない。Arduino IDE 2.3.8 + CH55xduino で
     ビルド・書き込みし、実機確認が必要。
*/

#include <WS2812.h>

#define NUM_LEDS 1
#define COLOR_PER_LEDS 3
#define NUM_BYTES (NUM_LEDS * COLOR_PER_LEDS)
__xdata uint8_t ledData[NUM_BYTES];

#ifndef USER_USB_RAM
#error "This firmware needs the USER CODE USB setting (usb_settings=user266)"
#endif

#include "src/userQmkCompatibleKeyboard/USBHIDKeyboardMouse.h"
#include "src/userQmkCompatibleKeyboard/via.h"
#include "keyboardConfig.h"

#define BUTTON1_PIN 15
#define LED_BUILTIN 14

// ---- agent-key protocol constants ----------------------------------------
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

// ---- gesture timing (agentkey.ino / docs/PROTOCOL.md と同期) -------------
#define DEBOUNCE_MS 30
#define LONG_MS 800
#define VERYLONG_MS 3000
#define DOUBLE_WINDOW_MS 350

// HID keyboard interaction overlay. Keep this local to the HID firmware so the
// host-controlled agent-key status color remains the base state.
#define PRESS_RELEASE_GLOW_MS 260

// ---- current LED command (via.c 経由で host が更新) -----------------------
uint8_t curState = ST_IDLE;
uint8_t curRisk = 0;
uint8_t curPattern = PAT_BREATH;
uint8_t curBrightness = 40;

// ---- button state ----------------------------------------------------------
bool btnPrev = false;
unsigned long btnChangeMillis = 0;
unsigned long pressStartMillis = 0;
bool veryLongSent = false;
// 単押し確定待ち (ダブル判定ウィンドウ)
bool clickPending = false;
unsigned long clickPendingMillis = 0;
// HID key down をホストへ送ったか。承認待ち中は down を送らないが、
// down 送信後に承認待ちへ遷移しても release は必ず送るための状態
bool keyPressedToHost = false;

bool ledButtonPressed = false;
bool ledReleaseGlowActive = false;
unsigned long ledReleaseMillis = 0;

unsigned long lastRenderMillis = 0;

uint8_t layerInUse = 0;

// via.c から呼ばれる: A1 packet 受信 (checksum 検証済み)
void agentkey_handle_host_packet(uint8_t state, uint8_t risk, uint8_t pattern,
                                 uint8_t brightness) {
  curState = state;
  curRisk = risk;
  curPattern = pattern;
  curBrightness = brightness;
}

// ---- LED -------------------------------------------------------------------
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

  // Physical switch feedback for HID keyboard use:
  // - while pressed: brighten the current agent-key color
  // - just after release: a short white-tinted decay
  // This is only a visual overlay; it does not change the protocol state.
  uint8_t glow = 0;
  if (ledButtonPressed) {
    glow = curBrightness < 96 ? 96 : curBrightness;
  } else if (ledReleaseGlowActive) {
    unsigned long elapsed = now - ledReleaseMillis;
    if (elapsed < PRESS_RELEASE_GLOW_MS) {
      uint16_t remain = PRESS_RELEASE_GLOW_MS - elapsed;
      uint8_t maxGlow = curBrightness < 96 ? 96 : curBrightness;
      glow = (uint8_t)((uint32_t)maxGlow * remain / PRESS_RELEASE_GLOW_MS);
    } else {
      ledReleaseGlowActive = false;
    }
  }
  if (glow > 0) {
    uint16_t warmR = glow;
    uint16_t warmG = (uint16_t)glow * 220 / 255;
    uint16_t warmB = (uint16_t)glow * 120 / 255;
    r = (uint8_t)(r + ((255 - r) * warmR / 255));
    g = (uint8_t)(g + ((255 - g) * warmG / 255));
    b = (uint8_t)(b + ((255 - b) * warmB / 255));
  }

  // Helper accepts RGB arguments and stores them in GRB byte order.
  set_pixel_for_GRB_LED(ledData, 0, r, g, b);
  neopixel_show_P1_7(ledData, NUM_BYTES);
}

// ---- button ------------------------------------------------------------------
void checkButton(unsigned long now) {
  // 単押し確定: ダブル判定ウィンドウが閉じた
  if (clickPending && (now - clickPendingMillis) >= DOUBLE_WINDOW_MS) {
    clickPending = false;
    agentkey_send_event(EV_SINGLE);
  }

  if ((now - btnChangeMillis) < DEBOUNCE_MS) return; // naive debouncing

  bool pressed = !digitalRead(BUTTON1_PIN);

  // 押下中に超長押し閾値を跨いだら即時に緊急停止イベント
  if (pressed && btnPrev && !veryLongSent && (now - pressStartMillis) >= VERYLONG_MS) {
    veryLongSent = true;
    agentkey_send_event(EV_VERYLONG);
  }

  if (pressed == btnPrev) return;
  btnPrev = pressed;
  btnChangeMillis = now;

  // via キーマップのキーも生かす (デフォルトは KEY_NONE で無入力)。
  // 承認待ち中 (ST_APPROVAL) は key down を送らず、承認クリックが
  // HIDキー入力として漏れないようにする。key down 済みなら、途中で
  // 承認待ちに遷移していても release は必ず送る (押しっぱなし防止)
  if (pressed) {
    if (curState != ST_APPROVAL) {
      press_qmk_key(0, 0, layerInUse, true);
      keyPressedToHost = true;
    }
  } else if (keyPressedToHost) {
    press_qmk_key(0, 0, layerInUse, false);
    keyPressedToHost = false;
  }
  // 押下の視覚フィードバック (LEDのみ。HID抑止やプロトコル状態には影響しない)
  ledButtonPressed = pressed;
  if (!pressed) {
    ledReleaseGlowActive = true;
    ledReleaseMillis = now;
  }

  if (pressed) {
    pressStartMillis = now;
    veryLongSent = false;
    agentkey_send_event(EV_DOWN);
  } else {
    agentkey_send_event(EV_UP);
    unsigned long dur = now - pressStartMillis;
    if (veryLongSent) {
      // 超長押しは押下中に送信済み
    } else if (dur >= LONG_MS) {
      agentkey_send_event(EV_LONG);
    } else if (clickPending) {
      // ウィンドウ内の2回目の短押し -> ダブル
      clickPending = false;
      agentkey_send_event(EV_DOUBLE);
    } else {
      clickPending = true;
      clickPendingMillis = now;
    }
  }
}

// ---- init ---------------------------------------------------------------------
void eeprom_init() {
  __data uint8_t allConfigFF = 1;
  __data uint8_t dataLength = ROWS_COUNT * COLS_COUNT * LAYER_COUNT * 2;
  for (__data uint8_t i = 0; i < dataLength; i++) {
    if (eeprom_read_byte(i) != 0xFF) {
      allConfigFF = 0;
      break;
    }
  }
  // eepromが空なら初期キーマップを書き込む。agent-key 用途では承認クリックの
  // たびに文字が入力されないよう KEY_NONE をデフォルトにする (1key.ino は TAB)。
  if (allConfigFF) {
    const uint8_t defaultKeymap[] = {KEY_NONE, KEY_NONE};
    for (__data uint8_t i = 0; i < dataLength; i++) {
      eeprom_write_byte(i, defaultKeymap[i]);
    }
  }
}

void setup() {
  USBInit();
  eeprom_init();
  pinMode(BUTTON1_PIN, INPUT_PULLUP);
  pinMode(LED_BUILTIN, OUTPUT);
  digitalWrite(LED_BUILTIN, HIGH);
  // ホスト側が接続を検出できるように起床通知
  delay(100);
  agentkey_send_event(EV_READY);
}

void loop() {
  unsigned long now = millis();
  via_process();     // raw HID: via コマンド + agent-key A1 packet
  checkButton(now);
  if ((now - lastRenderMillis) >= 16) { // ~60fps
    lastRenderMillis = now;
    renderLed(now);
  }
}

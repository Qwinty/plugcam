//! Which language the app speaks, and the few strings the backend shows itself (tray menu).
//! The window's strings live in app/src/lib/locales.

/// Same codes as `LANGUAGES` in app/src/lib/i18n.ts.
pub const SUPPORTED_LANGUAGES: &[&str] =
    &["en", "de", "es", "fr", "it", "pl", "pt-BR", "tr", "uk", "ru", "ja", "ko", "zh-CN"];

/// The language to use: the user's choice if it is supported, else the closest match for a
/// Windows locale like `pt-PT` or `zh-Hans-CN`, else English.
pub fn resolve(choice: Option<&str>, system_locale: &str) -> &'static str {
    if let Some(code) = choice.and_then(|c| SUPPORTED_LANGUAGES.iter().find(|s| s.eq_ignore_ascii_case(c))) {
        return code;
    }
    let primary = system_locale.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    match primary.as_str() {
        "pt" => "pt-BR",
        "zh" => "zh-CN",
        // Belarusian users usually read Russian better than English.
        "be" => "ru",
        p => SUPPORTED_LANGUAGES.iter().find(|s| **s == p).copied().unwrap_or("en"),
    }
}

pub struct TrayTexts {
    pub on: &'static str,
    pub off: &'static str,
    pub open: &'static str,
    pub quit: &'static str,
    pub tooltip_on: &'static str,
    pub tooltip_off: &'static str,
}

pub fn tray(language: &str) -> TrayTexts {
    let [on, off, open, quit, tooltip_on, tooltip_off] = match language {
        "ru" => ["Включить камеру", "Выключить камеру", "Открыть Plugcam", "Выход", "Plugcam — камера включена", "Plugcam — камера выключена"],
        "uk" => ["Увімкнути камеру", "Вимкнути камеру", "Відкрити Plugcam", "Вихід", "Plugcam — камеру увімкнено", "Plugcam — камеру вимкнено"],
        "de" => ["Kamera einschalten", "Kamera ausschalten", "Plugcam öffnen", "Beenden", "Plugcam – Kamera an", "Plugcam – Kamera aus"],
        "fr" => ["Allumer la caméra", "Éteindre la caméra", "Ouvrir Plugcam", "Quitter", "Plugcam — caméra allumée", "Plugcam — caméra éteinte"],
        "es" => ["Encender cámara", "Apagar cámara", "Abrir Plugcam", "Salir", "Plugcam: cámara encendida", "Plugcam: cámara apagada"],
        "pt-BR" => ["Ligar câmera", "Desligar câmera", "Abrir o Plugcam", "Sair", "Plugcam — câmera ligada", "Plugcam — câmera desligada"],
        "it" => ["Accendi fotocamera", "Spegni fotocamera", "Apri Plugcam", "Esci", "Plugcam — fotocamera accesa", "Plugcam — fotocamera spenta"],
        "pl" => ["Włącz kamerę", "Wyłącz kamerę", "Otwórz Plugcam", "Zakończ", "Plugcam — kamera włączona", "Plugcam — kamera wyłączona"],
        "tr" => ["Kamerayı aç", "Kamerayı kapat", "Plugcam’i aç", "Çıkış", "Plugcam — kamera açık", "Plugcam — kamera kapalı"],
        "zh-CN" => ["打开摄像头", "关闭摄像头", "打开 Plugcam", "退出", "Plugcam — 摄像头已打开", "Plugcam — 摄像头已关闭"],
        "ja" => ["カメラをオン", "カメラをオフ", "Plugcam を開く", "終了", "Plugcam — カメラ オン", "Plugcam — カメラ オフ"],
        "ko" => ["카메라 켜기", "카메라 끄기", "Plugcam 열기", "종료", "Plugcam — 카메라 켜짐", "Plugcam — 카메라 꺼짐"],
        _ => ["Turn camera on", "Turn camera off", "Open Plugcam", "Quit", "Plugcam — camera on", "Plugcam — camera off"],
    };
    TrayTexts { on, off, open, quit, tooltip_on, tooltip_off }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_choice_then_system() {
        assert_eq!(resolve(Some("de"), "ru-RU"), "de");
        assert_eq!(resolve(Some("pt-br"), "en-US"), "pt-BR");
        assert_eq!(resolve(Some("xx"), "ru-RU"), "ru");
        assert_eq!(resolve(None, "pt-PT"), "pt-BR");
        assert_eq!(resolve(None, "zh-Hans-CN"), "zh-CN");
        assert_eq!(resolve(None, "ja_JP"), "ja");
        assert_eq!(resolve(None, "nl-NL"), "en");
        assert_eq!(resolve(None, ""), "en");
    }

    #[test]
    fn every_language_has_tray_texts() {
        for code in SUPPORTED_LANGUAGES.iter().filter(|c| **c != "en") {
            assert_ne!(tray(code).quit, "Quit", "{code}");
        }
    }
}

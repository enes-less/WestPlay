; WestPlay's own steps in Tauri's installer (`bundle.windows.nsis.installerHooks`, tauri.release.conf.json).
; Included before the installer's languages, so its words are picked by $LANGUAGE as it runs.

!include LogicLib.nsh

; The WebView2 Runtime's id with EdgeUpdate (the installer's own `WEBVIEW2APPGUID`, defined after this file;
; a test gives another, one no PC has).
!ifndef WESTPLAY_WEBVIEW2
  !define WESTPLAY_WEBVIEW2 "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
!endif
; Ours, so the installer's own registers stay as they were.
Var WestPlayWebView2
Var WestPlayMessage
Var WestPlayVersion
Var WestPlayChannel
Var WestPlayGuid

; Edge's preview channels, which WebView2 also runs on when the Runtime isn't there: Beta, Dev, Canary.
!define WESTPLAY_EDGE_BETA "{2CD8A007-E189-409D-A2C8-9AF4EF3C72AA}"
!define WESTPLAY_EDGE_DEV "{0D50BFEC-CD6A-4F9A-964C-C7416E3ACB10}"
!define WESTPLAY_EDGE_CANARY "{65C35B14-6C1D-4122-AC46-7148CC9D6497}"
!define WESTPLAY_WEBVIEW2_PAGE "https://developer.microsoft.com/microsoft-edge/webview2/"

; Whether WebView2 can run here, in $WestPlayWebView2 ("found", or empty): the Runtime's version as
; EdgeUpdate registers it (for the machine, else for this user) with its program in that version's folder, or
; one of Edge's preview channels registered. The registry alone isn't enough: a PC whose WebView2 files were taken away (by a tool
; removing Edge) keeps its entry, which the installer's own WebView2 step takes as installed (a player's PC,
; 2026-10-24: WestPlay then stopped as it started, "failed to create webview", 0x80070002).
Function WestPlayFindWebView2
  StrCpy $WestPlayWebView2 ""
  ; The 32-bit view: on 64-bit Windows, WOW6432Node, where the Runtime registers.
  ReadRegStr $WestPlayVersion HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WESTPLAY_WEBVIEW2}" "pv"
  ${If} $WestPlayVersion != ""
  ${AndIf} ${FileExists} "$PROGRAMFILES32\Microsoft\EdgeWebView\Application\$WestPlayVersion\msedgewebview2.exe"
    StrCpy $WestPlayWebView2 "found"
    Return
  ${EndIf}
  ReadRegStr $WestPlayVersion HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WESTPLAY_WEBVIEW2}" "pv"
  ${If} $WestPlayVersion != ""
  ${AndIf} ${FileExists} "$LOCALAPPDATA\Microsoft\EdgeWebView\Application\$WestPlayVersion\msedgewebview2.exe"
    StrCpy $WestPlayWebView2 "found"
    Return
  ${EndIf}
  ${ForEach} $WestPlayChannel 1 3 + 1
    ${If} $WestPlayChannel = 1
      StrCpy $WestPlayGuid "${WESTPLAY_EDGE_BETA}"
    ${ElseIf} $WestPlayChannel = 2
      StrCpy $WestPlayGuid "${WESTPLAY_EDGE_DEV}"
    ${Else}
      StrCpy $WestPlayGuid "${WESTPLAY_EDGE_CANARY}"
    ${EndIf}
    ReadRegStr $WestPlayVersion HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\$WestPlayGuid" "pv"
    ${If} $WestPlayVersion == ""
      ReadRegStr $WestPlayVersion HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\$WestPlayGuid" "pv"
    ${EndIf}
    ${If} $WestPlayVersion != ""
    ${AndIf} $WestPlayVersion != "0.0.0.0"
      StrCpy $WestPlayWebView2 "found"
      Return
    ${EndIf}
  ${Next}
FunctionEnd

; The message when it can't, in $WestPlayMessage, in the installer's language (English for one not here).
Function WestPlayWebView2Message
  ${Select} $LANGUAGE
    ${Case} 1055 ; Turkish
      StrCpy $WestPlayMessage "WestPlay için Microsoft Edge WebView2 Runtime gerekiyor; bu bilgisayarda çalışmıyor (eksik ya da bozuk). Microsoft'un sayfasından kurun, sonra WestPlay kurulumunu yeniden çalıştırın.$\n$\nMicrosoft'un sayfası şimdi açılsın mı?"
    ${Case} 1031 ; German
      StrCpy $WestPlayMessage "WestPlay braucht die Microsoft Edge WebView2 Runtime, und sie funktioniert auf diesem PC nicht (sie fehlt oder ist beschädigt). Installiere sie von Microsofts Seite und starte danach das WestPlay-Setup erneut.$\n$\nMicrosofts Seite jetzt öffnen?"
    ${Case} 1036 ; French
      StrCpy $WestPlayMessage "WestPlay a besoin de Microsoft Edge WebView2 Runtime, qui ne fonctionne pas sur ce PC (absent ou endommagé). Installez-le depuis la page de Microsoft, puis relancez l’installation de WestPlay.$\n$\nOuvrir la page de Microsoft maintenant ?"
    ${Case} 1034 ; Spanish
      StrCpy $WestPlayMessage "WestPlay necesita Microsoft Edge WebView2 Runtime, que no funciona en este PC (falta o está dañado). Instálalo desde la página de Microsoft y vuelve a ejecutar la instalación de WestPlay.$\n$\n¿Abrir ahora la página de Microsoft?"
    ${Case} 3082 ; Spanish (international)
      StrCpy $WestPlayMessage "WestPlay necesita Microsoft Edge WebView2 Runtime, que no funciona en este PC (falta o está dañado). Instálalo desde la página de Microsoft y vuelve a ejecutar la instalación de WestPlay.$\n$\n¿Abrir ahora la página de Microsoft?"
    ${Case} 1040 ; Italian
      StrCpy $WestPlayMessage "WestPlay ha bisogno di Microsoft Edge WebView2 Runtime, che su questo PC non funziona (manca o è danneggiato). Installalo dalla pagina di Microsoft, poi avvia di nuovo l’installazione di WestPlay.$\n$\nAprire ora la pagina di Microsoft?"
    ${Case} 1046 ; Portuguese (Brazil)
      StrCpy $WestPlayMessage "O WestPlay precisa do Microsoft Edge WebView2 Runtime, que não está funcionando neste PC (ausente ou danificado). Instale-o pela página da Microsoft e execute a instalação do WestPlay novamente.$\n$\nAbrir a página da Microsoft agora?"
    ${Case} 2070 ; Portuguese
      StrCpy $WestPlayMessage "O WestPlay precisa do Microsoft Edge WebView2 Runtime, que não está a funcionar neste PC (em falta ou danificado). Instale-o a partir da página da Microsoft e execute novamente a instalação do WestPlay.$\n$\nAbrir agora a página da Microsoft?"
    ${Case} 1043 ; Dutch
      StrCpy $WestPlayMessage "WestPlay heeft Microsoft Edge WebView2 Runtime nodig, en die werkt niet op deze pc (ontbreekt of is beschadigd). Installeer het via de pagina van Microsoft en start daarna de installatie van WestPlay opnieuw.$\n$\nDe pagina van Microsoft nu openen?"
    ${Case} 1045 ; Polish
      StrCpy $WestPlayMessage "WestPlay potrzebuje Microsoft Edge WebView2 Runtime, który nie działa na tym komputerze (brak go lub jest uszkodzony). Zainstaluj go ze strony Microsoftu, a potem uruchom instalację WestPlay ponownie.$\n$\nOtworzyć teraz stronę Microsoftu?"
    ${Case} 1049 ; Russian
      StrCpy $WestPlayMessage "WestPlay нужен Microsoft Edge WebView2 Runtime, а на этом компьютере он не работает (отсутствует или повреждён). Установите его со страницы Microsoft, затем снова запустите установку WestPlay.$\n$\nОткрыть страницу Microsoft сейчас?"
    ${Case} 1058 ; Ukrainian
      StrCpy $WestPlayMessage "WestPlay потрібен Microsoft Edge WebView2 Runtime, але на цьому комп’ютері він не працює (відсутній або пошкоджений). Встановіть його зі сторінки Microsoft, потім знову запустіть встановлення WestPlay.$\n$\nВідкрити сторінку Microsoft зараз?"
    ${Case} 1029 ; Czech
      StrCpy $WestPlayMessage "WestPlay potřebuje Microsoft Edge WebView2 Runtime, který na tomto počítači nefunguje (chybí nebo je poškozený). Nainstalujte ho ze stránky Microsoftu a pak spusťte instalaci WestPlay znovu.$\n$\nOtevřít stránku Microsoftu?"
    ${Case} 1051 ; Slovak
      StrCpy $WestPlayMessage "WestPlay potrebuje Microsoft Edge WebView2 Runtime, ktorý na tomto počítači nefunguje (chýba alebo je poškodený). Nainštalujte ho zo stránky Microsoftu a potom znova spustite inštaláciu WestPlay.$\n$\nOtvoriť stránku Microsoftu?"
    ${Case} 1026 ; Bulgarian
      StrCpy $WestPlayMessage "WestPlay се нуждае от Microsoft Edge WebView2 Runtime, който не работи на този компютър (липсва или е повреден). Инсталирайте го от страницата на Microsoft и след това стартирайте инсталацията на WestPlay отново.$\n$\nДа се отвори ли страницата на Microsoft сега?"
    ${Case} 1050 ; Croatian
      StrCpy $WestPlayMessage "WestPlay treba Microsoft Edge WebView2 Runtime, a on na ovom računalu ne radi (nedostaje ili je oštećen). Instalirajte ga sa stranice Microsofta, zatim ponovno pokrenite instalaciju WestPlaya.$\n$\nOtvoriti Microsoftovu stranicu sada?"
    ${Case} 1060 ; Slovenian
      StrCpy $WestPlayMessage "WestPlay potrebuje Microsoft Edge WebView2 Runtime, ki v tem računalniku ne deluje (manjka ali je poškodovan). Namestite ga s strani Microsofta in nato znova zaženite namestitev WestPlaya.$\n$\nŽelite zdaj odpreti Microsoftovo stran?"
    ${Case} 3098 ; Serbian (Cyrillic)
      StrCpy $WestPlayMessage "WestPlay-у је потребан Microsoft Edge WebView2 Runtime, а на овом рачунару не ради (недостаје или је оштећен). Инсталирајте га са Microsoft-ове странице, па поново покрените инсталацију WestPlay-а.$\n$\nОтворити Microsoft-ову страницу сада?"
    ${Case} 2074 ; Serbian (Latin)
      StrCpy $WestPlayMessage "WestPlay-u je potreban Microsoft Edge WebView2 Runtime, a na ovom računaru ne radi (nedostaje ili je oštećen). Instalirajte ga sa Microsoft-ove stranice, pa ponovo pokrenite instalaciju WestPlay-a.$\n$\nOtvoriti Microsoft-ovu stranicu sada?"
    ${Case} 1071 ; Macedonian
      StrCpy $WestPlayMessage "На WestPlay му треба Microsoft Edge WebView2 Runtime, а на овој компјутер не работи (недостасува или е оштетен). Инсталирајте го од страницата на Microsoft, па повторно стартувајте ја инсталацијата на WestPlay.$\n$\nДа се отвори ли страницата на Microsoft сега?"
    ${Case} 1030 ; Danish
      StrCpy $WestPlayMessage "WestPlay skal bruge Microsoft Edge WebView2 Runtime, og den virker ikke på denne pc (den mangler eller er beskadiget). Installer den fra Microsofts side, og kør derefter WestPlays installation igen.$\n$\nÅbn Microsofts side nu?"
    ${Case} 1053 ; Swedish
      StrCpy $WestPlayMessage "WestPlay behöver Microsoft Edge WebView2 Runtime, och den fungerar inte på den här datorn (saknas eller är skadad). Installera den från Microsofts sida och kör sedan WestPlays installation igen.$\n$\nÖppna Microsofts sida nu?"
    ${Case} 1044 ; Norwegian
      StrCpy $WestPlayMessage "WestPlay trenger Microsoft Edge WebView2 Runtime, og den virker ikke på denne PC-en (mangler eller er skadet). Installer den fra Microsofts side, og kjør deretter WestPlays installasjon på nytt.$\n$\nÅpne Microsofts side nå?"
    ${Case} 1035 ; Finnish
      StrCpy $WestPlayMessage "WestPlay tarvitsee Microsoft Edge WebView2 Runtimen, joka ei toimi tässä tietokoneessa (puuttuu tai on vioittunut). Asenna se Microsoftin sivulta ja suorita sitten WestPlayn asennus uudelleen.$\n$\nAvataanko Microsoftin sivu nyt?"
    ${Case} 1061 ; Estonian
      StrCpy $WestPlayMessage "WestPlay vajab Microsoft Edge WebView2 Runtime’i, mis selles arvutis ei tööta (puudub või on kahjustatud). Paigalda see Microsofti lehelt ja käivita seejärel WestPlay paigaldus uuesti.$\n$\nKas avada Microsofti leht kohe?"
    ${Case} 1062 ; Latvian
      StrCpy $WestPlayMessage "WestPlay nepieciešams Microsoft Edge WebView2 Runtime, bet šajā datorā tas nedarbojas (trūkst vai ir bojāts). Instalējiet to no Microsoft lapas un pēc tam vēlreiz palaidiet WestPlay instalēšanu.$\n$\nVai atvērt Microsoft lapu tagad?"
    ${Case} 1063 ; Lithuanian
      StrCpy $WestPlayMessage "WestPlay reikia Microsoft Edge WebView2 Runtime, bet šiame kompiuteryje jis neveikia (jo nėra arba jis sugadintas). Įdiekite jį iš Microsoft puslapio, tada vėl paleiskite WestPlay diegimą.$\n$\nAtidaryti Microsoft puslapį dabar?"
    ${Case} 1038 ; Hungarian
      StrCpy $WestPlayMessage "A WestPlayhez Microsoft Edge WebView2 Runtime kell, ami ezen a gépen nem működik (hiányzik vagy sérült). Telepítse a Microsoft oldaláról, majd futtassa újra a WestPlay telepítőjét.$\n$\nMegnyitja most a Microsoft oldalát?"
    ${Case} 1048 ; Romanian
      StrCpy $WestPlayMessage "WestPlay are nevoie de Microsoft Edge WebView2 Runtime, care nu funcționează pe acest PC (lipsește sau este deteriorat). Instalați-l de pe pagina Microsoft, apoi rulați din nou instalarea WestPlay.$\n$\nDeschideți acum pagina Microsoft?"
    ${Case} 1032 ; Greek
      StrCpy $WestPlayMessage "Το WestPlay χρειάζεται το Microsoft Edge WebView2 Runtime, που δεν λειτουργεί σε αυτόν τον υπολογιστή (λείπει ή είναι κατεστραμμένο). Εγκαταστήστε το από τη σελίδα της Microsoft και μετά εκτελέστε ξανά την εγκατάσταση του WestPlay.$\n$\nΆνοιγμα της σελίδας της Microsoft τώρα;"
    ${Case} 1027 ; Catalan
      StrCpy $WestPlayMessage "El WestPlay necessita el Microsoft Edge WebView2 Runtime, que no funciona en aquest PC (hi falta o està malmès). Instal·la’l des de la pàgina de Microsoft i torna a executar la instal·lació del WestPlay.$\n$\nVols obrir ara la pàgina de Microsoft?"
    ${Case} 1069 ; Basque
      StrCpy $WestPlayMessage "WestPlay-k Microsoft Edge WebView2 Runtime behar du, eta ordenagailu honetan ez dabil (falta da edo hondatuta dago). Instalatu Microsoft-en orritik, eta exekutatu berriro WestPlay-ren instalazioa.$\n$\nMicrosoft-en orria orain ireki?"
    ${Case} 1110 ; Galician
      StrCpy $WestPlayMessage "WestPlay necesita Microsoft Edge WebView2 Runtime, que non funciona neste PC (falta ou está danado). Instálao desde a páxina de Microsoft e volve executar a instalación de WestPlay.$\n$\nAbrir agora a páxina de Microsoft?"
    ${Case} 1079 ; Georgian
      StrCpy $WestPlayMessage "WestPlay-ს სჭირდება Microsoft Edge WebView2 Runtime, რომელიც ამ კომპიუტერზე არ მუშაობს (აკლია ან დაზიანებულია). დააინსტალირეთ Microsoft-ის გვერდიდან და შემდეგ თავიდან გაუშვით WestPlay-ის ინსტალაცია.$\n$\nგაიხსნას Microsoft-ის გვერდი ახლა?"
    ${Case} 1041 ; Japanese
      StrCpy $WestPlayMessage "WestPlay には Microsoft Edge WebView2 Runtime が必要ですが、この PC では動作していません（見つからないか、壊れています）。Microsoft のページからインストールしてから、WestPlay のセットアップをもう一度実行してください。$\n$\nMicrosoft のページを今すぐ開きますか？"
    ${Case} 1042 ; Korean
      StrCpy $WestPlayMessage "WestPlay에는 Microsoft Edge WebView2 Runtime이 필요하지만 이 PC에서 작동하지 않습니다(없거나 손상됨). Microsoft 페이지에서 설치한 다음 WestPlay 설치를 다시 실행하세요.$\n$\n지금 Microsoft 페이지를 열까요?"
    ${Case} 2052 ; Chinese (Simplified)
      StrCpy $WestPlayMessage "WestPlay 需要 Microsoft Edge WebView2 Runtime，但它在这台电脑上无法运行（缺失或已损坏）。请从 Microsoft 的页面安装它，然后重新运行 WestPlay 安装程序。$\n$\n现在打开 Microsoft 的页面吗？"
    ${Case} 1028 ; Chinese (Traditional)
      StrCpy $WestPlayMessage "WestPlay 需要 Microsoft Edge WebView2 Runtime，但它在這台電腦上無法執行（缺少或已損毀）。請從 Microsoft 的網頁安裝，然後重新執行 WestPlay 安裝程式。$\n$\n現在開啟 Microsoft 的網頁嗎？"
    ${Case} 1066 ; Vietnamese
      StrCpy $WestPlayMessage "WestPlay cần Microsoft Edge WebView2 Runtime, nhưng nó không chạy trên máy tính này (bị thiếu hoặc bị hỏng). Hãy cài đặt từ trang của Microsoft, rồi chạy lại trình cài đặt WestPlay.$\n$\nMở trang của Microsoft ngay bây giờ?"
    ${CaseElse}
      StrCpy $WestPlayMessage "WestPlay needs Microsoft Edge WebView2 Runtime, and it isn't working on this PC (it's missing or damaged). Install it from Microsoft's page, then run WestPlay's setup again.$\n$\nOpen Microsoft's page now?"
  ${EndSelect}
FunctionEnd

; Before anything is installed: WestPlay's windows are WebView2's, so with no WebView2 that can run it can't
; start at all. Setup then says so, offers Microsoft's page, and stops (owner 2026-10-24: "so it does not
; continue to install"). Silent, it stops without asking.
!macro NSIS_HOOK_PREINSTALL
  Call WestPlayFindWebView2
  ${If} $WestPlayWebView2 == ""
    Call WestPlayWebView2Message
    MessageBox MB_ICONSTOP|MB_YESNO "$WestPlayMessage" /SD IDNO IDNO +2
    ExecShell "open" "${WESTPLAY_WEBVIEW2_PAGE}"
    Abort "WebView2 Runtime: missing or damaged"
  ${EndIf}
!macroend

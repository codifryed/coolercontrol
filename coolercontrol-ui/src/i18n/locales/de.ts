/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

export default {
    common: {
        save: 'Speichern',
        cancel: 'Abbrechen',
        confirm: 'Bestätigen',
        delete: 'Löschen',
        edit: 'Bearbeiten',
        add: 'Hinzufügen',
        remove: 'Entfernen',
        yes: 'Ja',
        no: 'Nein',
        ok: 'OK',
        error: 'Fehler',
        success: 'Erfolg',
        warning: 'Warnung',
        loading: 'Lade...',
        restarting: 'Neustart...',
        noData: 'Keine Daten verfügbar',
        retry: 'Wiederholen',
        saveAndRefresh: 'Speichern und aktualisieren',
        reset: 'Zurücksetzen',
        back: 'Zurück',
        sslTls: 'SSL/TLS',
        protocol: 'Protokoll',
        address: 'Adresse',
        port: 'Port',
        search: 'Suchen',
        selected: 'Ausgewählt',
        clear: 'Löschen',
        finish: 'Fertigstellen',
        next: 'Weiter',
        previous: 'Zurück',
        apply: 'Anwenden',
        defaults: 'Standardwerte',
        rename: 'Umbenennen',
        password: 'Passwort',
        savePassword: 'Passwort speichern',
        editName: 'Namen bearbeiten',
        state: 'Status',
        name: 'Name',
        message: 'Nachricht',
        timestamp: 'Zeitstempel',
        overview: 'Übersicht',
        login: 'Anmelden',
        logout: 'Abmelden',
        temperature: 'Temperatur',
        duty: 'Leistung',
        stay: 'Bleiben',
        discard: 'Verwerfen',
        blankNameResetDefault: 'Ein leerer Name setzt den Wert auf den Systemstandard zurück.',
        copy: '(Kopie)',
        minuteAbbr: 'Min',
        rpmAbbr: 'U/min',
        mhzAbbr: 'MHz',
        ghzAbbr: 'GHz',
        tempUnit: '°C',
        percentUnit: '%',
        secondAbbr: 's',
        toast: {
            modeCreated: 'Modus erstellt',
            modeDuplicated: 'Modus dupliziert',
            modeNameUpdated: 'Modusname aktualisiert',
            modeUpdated: 'Modus mit aktuellen Einstellungen aktualisiert',
            modeDeleted: 'Modus gelöscht',
            modeActivated: 'Modus aktiviert',
            customSensorSaved: 'Benutzerdefinierter Sensor gespeichert und UI wird aktualisiert...',
            customSensorUpdated:
                'Benutzerdefinierter Sensor erfolgreich aktualisiert und UI wird aktualisiert...',
            customSensorDeleted:
                'Benutzerdefinierter Sensor erfolgreich gelöscht und UI wird aktualisiert...',
            alertSaved: 'Alarm gespeichert',
            alertUpdated: 'Alarm aktualisiert',
            alertDeleted: 'Alarm gelöscht',
            alertNotFound: 'Zu aktualisierender Alarm nicht gefunden',
            settingsUpdated: 'Einstellungen erfolgreich aktualisiert und auf das Gerät angewendet',
            settingsError:
                'Beim Versuch, diese Einstellungen anzuwenden, ist ein Fehler aufgetreten',
            thinkPadFanControlApplied: 'ThinkPad-Lüftersteuerung erfolgreich angewendet',
        },
    },
    layout: {
        topbar: {
            login: 'Anmelden',
            logout: 'Abmelden',
            changePassword: 'Passwort ändern',
            restartUI: 'UI neustarten',
            restartDaemonAndUI: 'Daemon und UI neustarten',
            restartConfirmMessage:
                'Sind Sie sicher, dass Sie den Daemon und die UI neustarten möchten?',
            restartConfirmHeader: 'Daemon Neustart',
            shutdownSuccess: 'Daemon-Shutdown-Signal akzeptiert',
            shutdownError:
                'Unbekannter Fehler beim Senden des Shutdown-Signals. Details finden Sie in den Logs.',
            quitDesktopApp: 'Desktop-App beenden',
            applicationInfo: 'Anwendungsinfo',
            back: 'Zurück',
            expandMenu: 'Menü erweitern',
            collapseMenu: 'Menü einklappen',
            controls: 'Steuerungen',
            alerts: 'Warnungen',
            settings: 'Einstellungen',
            openInBrowser: 'Im Browser öffnen',
            modes: 'Modi',
            loginSuccessful: 'Anmeldung erfolgreich',
        },
        settings: {
            title: 'Einstellungen',
            general: 'Allgemein',
            device: 'Geräte',
            daemon: 'Daemon',
            thinkpad: 'ThinkPad',
            devices: {
                devicesAndSensors: 'Geräte und Sensoren',
                detectionIssues: 'Erkennungsprobleme? Siehe',
                hardwareSupportDoc: 'Hardware-Support-Dokumentation',
                toggleRequiresRestart:
                    'Das Umschalten von Geräten oder Sensoren erfordert einen Neustart des Daemons und der UI. Möchten Sie das jetzt tun?',
                enableDevices: 'Geräte aktivieren',
                selectTooltip:
                    'Wählen Sie Geräte und Sensoren aus, um sie zu deaktivieren oder zu aktivieren.\nDas Deaktivieren ungenutzter Geräte und Sensoren wird dringend empfohlen.',
                unknownError:
                    'Unbekannter Fehler beim Versuch, Änderungen auf alle Geräte anzuwenden. Details finden Sie in den Logs.',
            },
            profiles: 'Profile',
            alerts: 'Warnungen',
            dashboards: 'Dashboards',
            modes: 'Modi',
            appearance: 'Erscheinungsbild',
            language: 'Sprache',
            selectLanguage: 'Sprache auswählen',
            english: 'Englisch',
            chinese: 'Chinesisch (Vereinfacht)',
            japanese: 'Japanisch',
            chineseTrad: 'Chinesisch (Traditionell)',
            russian: 'Russisch',
            german: 'Deutsch',
            french: 'Französisch',
            spanish: 'Spanisch',
            arabic: 'Arabisch',
            portuguese: 'Portugiesisch',
            hindi: 'Hindi',
            theme: 'Theme',
            themeLight: 'Hell',
            themeDark: 'Dunkel',
            themeSystem: 'System',
            themeCustom: 'Benutzerdefiniert',
            themeHighContrastDark: 'Hoher Kontrast Dunkel',
            themeHighContrastLight: 'Hoher Kontrast Hell',
            lineThickness: 'Linienstärke',
            fullScreen: 'Vollbild',
            menuBarAlwaysVisible: 'Menüleiste immer sichtbar',
            hideMenuCollapseIcon: 'Menü-Einklapp-Symbol ausblenden',
            showOnboarding: 'Einführungstour beim Start anzeigen',
            introduction: 'Einführung',
            startTour: 'Tour starten',
            timeFormat: 'Zeitformat',
            time24h: '24 Stunden',
            time12h: '12 Stunden',
            frequencyPrecision: 'Frequenzgenauigkeit',
            sidebarToCollapse: 'Seitenleiste zum Einklappen',
            entitiesBelowSensors: 'Entitäten unter Sensoren',
            dashboardLineSize: 'Dashboard-Liniengröße',
            themeStyle: 'Theme-Stil',
            desktop: 'Desktop',
            startInTray: 'Im Tray starten',
            closeToTray: 'In Tray minimieren',
            zoom: 'Zoom',
            desktopStartupDelay: 'Desktop-Startverzögerung',
            fanControl: 'Lüftersteuerung',
            fullSpeed: 'Volle Geschwindigkeit',
            applySettingsOnStartup: 'Einstellungen beim Start anwenden',
            deviceDelayAtStartup: 'Geräteverzögerung beim Start',
            pollingRate: 'Abfragerate',
            compressApiPayload: 'API-Payload komprimieren',
            liquidctlIntegration: 'Liquidctl-Integration',
            liquidctlDeviceInit: 'Liquidctl-Geräteinitialisierung',
            hideDuplicateDevices: 'Doppelte Geräte ausblenden',
            drivePowerState: 'Festplattenstromzustand',
            themeMode: {
                system: 'System',
                dark: 'Dunkel',
                light: 'Hell',
                highContrastDark: 'Hoher Kontrast Dunkel',
                highContrastLight: 'Hoher Kontrast Hell',
                custom: 'Benutzerdefiniert',
            },
            customTheme: {
                title: 'Benutzerdefiniertes Theme',
                accent: 'Akzentfarbe',
                bgOne: 'Hintergrund Primär',
                bgTwo: 'Hintergrund Sekundär',
                border: 'Rahmenfarbe',
                text: 'Textfarbe',
                textSecondary: 'Sekundäre Textfarbe',
            },
            tooltips: {
                introduction: 'Die Einführungstour der Anwendung starten.',
                timeFormat: 'Zeitformat: 12-Stunden (AM/PM) oder 24-Stunden',
                frequencyPrecision:
                    'Stellen Sie die Genauigkeit der angezeigten Frequenzwerte ein.',
                sidebarCollapse:
                    'Ob ein Menü-Einklapp-Symbol in der Seitenleiste angezeigt werden soll,\noder der leere Bereich der Seitenleiste genutzt wird, um das Hauptmenü zu erweitern oder einzuklappen.',
                entitiesBelowSensors:
                    'Ob Entitäten unter Gerätesensoren im Hauptmenü angezeigt werden sollen.',
                fullScreen: 'Schaltet den Vollbildmodus ein oder aus',
                lineThickness: 'Passen Sie die Linienstärke der Diagramme im Dashboard an',
                startInTray:
                    'Beim Start wird das Hauptfenster der Benutzeroberfläche ausgeblendet und nur\ndas Symbol im Systemtray ist sichtbar.',
                closeToTray:
                    'Wenn Sie das Anwendungsfenster schließen, bleibt die App im Systemtray aktiv',
                zoom: 'Legen Sie manuell den Zoom-Level der Benutzeroberfläche fest.',
                desktopStartupDelay:
                    'Fügt eine Verzögerung vor dem Start der Desktop-Anwendung hinzu (in Sekunden).\nHilft bei Problemen, die dadurch entstehen, dass die Desktop-Anwendung\nautomatisch beim Login gestartet wird oder zu schnell startet',
                thinkPadFanControl:
                    'Dies ist ein Hilfsmittel zum Aktivieren der ThinkPad ACPI-Lüftersteuerung.\nLüftersteuerungsoperationen sind aus Sicherheitsgründen standardmäßig deaktiviert. CoolerControl kann versuchen, dies für Sie zu aktivieren, aber Sie sollten sich der Risiken für Ihre Hardware bewusst sein.\nFahren Sie auf eigenes Risiko fort.',
                thinkPadFullSpeed:
                    'Für ThinkPad-Laptops aktiviert dies den Vollgeschwindigkeitsmodus.\nDies ermöglicht es den Lüftern, bei Einstellung auf 100% auf ihr absolutes Maximum hochzudrehen, wird die Lüfter jedoch außerhalb der Spezifikation betreiben und zu erhöhtem Verschleiß führen.\nVerwenden Sie es mit Vorsicht.',
                applySettingsOnStartup:
                    'Einstellungen automatisch beim Daemon-Start und beim Aufwachen aus dem Ruhezustand anwenden',
                deviceDelayAtStartup:
                    'Verzögerung vor dem Start der Gerätekommunikation (in Sekunden).\nHilft bei Geräten, die Zeit zur Initialisierung benötigen oder nicht durchgängig erkannt werden',
                pollingRate:
                    'Die Rate, mit der Sensordaten abgefragt werden (in Sekunden).\nEine höhere Abfragerate reduziert die Ressourcennutzung, und eine niedrigere erhöht die Reaktionsfähigkeit.\nEine Rate von weniger als 1,0 sollte mit Vorsicht verwendet werden.',
                compressApiPayload:
                    'Aktivieren Sie die Antwortkomprimierung, um die API-Payload-Größe zu reduzieren,\nbeachten Sie jedoch, dass dies die CPU-Auslastung erhöht.',
                liquidctlIntegration:
                    'Durch Deaktivieren wird die Liquidctl-Integration vollständig deaktiviert, \nunabhängig vom Installationsstatus des coolercontrol-liqctld \nPakets. Falls verfügbar, werden stattdessen HWMon-Treiber verwendet.',
                liquidctlDeviceInit:
                    'Vorsicht: Deaktivieren Sie dies NUR, wenn Sie oder ein anderes Programm\ndie liquidctl-Geräteinitialisierung handhaben.\nDies kann helfen, Konflikte mit anderen Programmen zu vermeiden.',
                hideDuplicateDevices:
                    'Einige Geräte werden sowohl von Liquidctl- als auch von HWMon-Treibern unterstützt.\nLiquidctl wird standardmäßig wegen seiner zusätzlichen Funktionen verwendet. Um stattdessen HWMon-Treiber zu verwenden,\ndeaktivieren Sie dies und das Liquidctl-Gerät, um Treiberkonflikte zu vermeiden.',
                drivePowerState:
                    'SSDs und HDDs können insbesondere heruntergefahren werden und in einen Energiesparmodus eintreten.\nDiese Option, wenn sie aktiviert ist und das Laufwerk dies unterstützt, wird die Laufwerktemperaturen\nals 0 °C melden, wenn es heruntergefahren ist, damit die Lüfterprofile entsprechend angepasst werden können.',
                daemonAddress:
                    'Die IP-Adresse oder der Domainname des Daemons, mit dem eine Verbindung hergestellt werden soll.\nUnterstützt IPv4, IPv6 und per DNS auflösbare Hostnamen.',
                daemonPort: 'Der Port, der für die Verbindung mit dem Daemon verwendet wird.',
                sslTls: 'Ob eine Verbindung zum Daemon über SSL/TLS hergestellt werden soll.\nEin Proxy-Setup ist erforderlich.',
                triggersRestart: 'Löst einen automatischen Neustart aus',
                triggersUIRestart: 'Löst einen automatischen UI-Neustart aus',
                triggersDaemonRestart: 'Löst einen automatischen Daemon-Neustart aus',
                resetToDefaults: 'Auf Standardeinstellungen zurücksetzen',
                saveAndReload: 'Speichern und die UI neu laden',
            },
            applySettingAndRestart:
                'Das Ändern dieser Einstellung erfordert einen Neustart des Daemons und der UI. Möchten Sie das jetzt tun?',
            restartHeader: 'Einstellung anwenden und Neustart',
            restartSuccess: 'Neustart läuft',
            success: 'Erfolg',
            successDetail: 'Operation erfolgreich abgeschlossen',
            settingsAppliedSuccess: 'Einstellungen erfolgreich angewendet',
            restartRequestSuccess: 'Neustart-Anfrage erfolgreich gesendet',
            colorPickerDialogTitle: 'Farbe auswählen',
            colorPickerConfirm: 'Bestätigen',
            colorPickerCancel: 'Abbrechen',
            languageChangeConfirm: 'Sprache ändern?',
            languageChangeConfirmMessage:
                'Sind Sie sicher, dass Sie fortfahren möchten? Wenn einige Oberflächenelemente nicht korrekt angezeigt werden, aktualisieren Sie bitte die Seite manuell.',
            languageChangeSuccess: 'Sprache erfolgreich gewechselt.',
            languageChangeError: 'Fehler beim Ändern der Sprache. Bitte versuchen Sie es erneut.',
            themeChangeSuccess: 'Theme erfolgreich gewechselt.',
            entitiesBelowSensorsEnabledMessage: 'Entitäten werden nun unter Sensoren angezeigt.',
            entitiesBelowSensorsDisabledMessage:
                'Entitäten werden nicht mehr unter Sensoren angezeigt.',
        },
        menu: {
            system: 'System',
            dashboards: 'Dashboards',
            profiles: 'Profile',
            functions: 'Funktionen',
            customSensors: 'Benutzerdefinierte Sensoren',
            modes: 'Modi',
            alerts: 'Warnungen',
            tooltips: {
                delete: 'Löschen',
                createMode: 'Modus aus aktuellen Einstellungen erstellen',
                addProfile: 'Profil hinzufügen',
                editName: 'Namen bearbeiten',
                addAlert: 'Warnung hinzufügen',
                deleteFunction: 'Funktion löschen',
                addDashboard: 'Dashboard hinzufügen',
                deleteDashboard: 'Dashboard löschen',
                duplicate: 'Duplizieren',
                setAsHome: 'Als Startseite festlegen',
                save: 'Speichern',
                deleteMode: 'Modus löschen',
                updateWithCurrentSettings: 'Mit aktuellen Einstellungen aktualisieren',
                rename: 'Umbenennen',
                createModeFromCurrentSettings: 'Modus aus aktuellen Einstellungen erstellen',
                addCustomSensor: 'Benutzerdefinierten Sensor hinzufügen',
                addFunction: 'Funktion hinzufügen',
                chooseColor: 'Farbe wählen',
            },
        },
        add: {
            dashboard: 'Dashboard',
            mode: 'Modus',
            profile: 'Profil',
            function: 'Funktion',
            alert: 'Warnung',
            customSensor: 'Benutzerdefinierter Sensor',
        },
    },
    views: {
        daemon: {
            title: 'Daemon',
            daemonErrors: 'Daemon-Fehler',
            daemonErrorsDetail:
                'Der Daemon hat Fehler gemeldet. Überprüfen Sie die Logs für Details.',
            daemonDisconnected: 'Daemon getrennt',
            daemonDisconnectedDetail:
                'Verbindung zum Daemon nicht möglich. Bitte überprüfen Sie, ob der Daemon läuft.',
            connectionRestored: 'Verbindung wiederhergestellt',
            connectionRestoredMessage: 'Die Verbindung zum Daemon wurde wiederhergestellt.',
            thinkpadFanControl: 'ThinkPad-Lüftersteuerung',
            pollRate: 'Abfragerate',
            applySettingAndRestart: 'Einstellung anwenden und Neustart',
            changeSetting:
                'Das Ändern dieser Einstellung erfordert einen Neustart des Daemons und der UI. Möchten Sie das jetzt tun?',
            status: {
                ok: 'Ok',
                hasWarnings: 'Hat Warnungen',
                hasErrors: 'Hat Fehler',
            },
        },
        devices: {
            detectionIssues: 'Erkennungsprobleme? Siehe die',
            hardwareSupportDocs: 'Hardware-Support-Dokumentation',
            selectDevices:
                'Wählen Sie Geräte und Sensoren aus, um sie zu deaktivieren oder zu aktivieren.\nDas Deaktivieren ungenutzter Geräte und Sensoren wird dringend empfohlen.',
            devicesAndSensors: 'Geräte und Sensoren',
            apply: 'Anwenden',
            applySettingsAndReload: 'Einstellungen anwenden und neu laden',
            triggersAutoRestart: 'Löst automatischen Daemon-Neustart aus',
            restartPrompt:
                'Das Umschalten von Geräten oder Sensoren erfordert einen Neustart des Daemons und der UI. Möchten Sie das jetzt tun?',
            enableDevices: 'Geräte aktivieren',
        },
        speed: {
            automatic: 'Automatisch',
            manual: 'Manuell',
            unsavedChanges: 'Ungespeicherte Änderungen',
            unsavedChangesMessage: 'Es gibt ungespeicherte Änderungen an diesem Steuerungskanal.',
            manualDuty: 'Manuelle Auslastung',
            profileToApply: 'Anzuwendendes Profil',
            automaticOrManual: 'Automatisch oder Manuell',
            driverNoSupportControl:
                'Der aktuell installierte Treiber unterstützt nicht die Steuerung dieses Kanals.',
            controlOrView: 'Steuern oder Anzeigen',
            applySetting: 'Einstellung anwenden',
        },
        customSensors: {
            newSensor: 'Neuer Sensor',
            sensorType: 'Sensortyp',
            type: 'Typ',
            mixFunction: 'Mix-Funktion',
            howCalculateValue: 'Wie der resultierende Sensorwert berechnet werden soll',
            tempFileLocation: 'Temp-Datei-Standort',
            tempFile: 'Temperaturdatei',
            filePathTooltip:
                'Geben Sie den absoluten Pfad zur Temperaturdatei ein, die für diesen Sensor verwendet werden soll.\nDie Datei muss das sysfs-Datenformat-Standard verwenden:\nEine Festkommazahl in Milligrad Celsius.\nz.B. 80000 für 80°C.\nDie Datei wird bei der Übermittlung überprüft.',
            browse: 'Durchsuchen',
            browseCustomSensorFile: 'Nach einer benutzerdefinierten Sensordatei suchen',
            tempSources: 'Temp-Quellen',
            tempSourcesTooltip:
                'Temperaturquellen, die in der Mix-Funktion verwendet werden sollen<br/><i>Hinweis: Sie können ein Mix-Profil verwenden, um mehrere<br/>benutzerdefinierte Sensoren zu kombinieren.</i>',
            tempWeights: 'Temp-Gewichtungen',
            tempWeightsTooltip: 'Die individuelle Gewichtung jeder ausgewählten Temperaturquelle.',
            tempName: 'Temp-Name',
            weight: 'Gewichtung',
            saveSensor: 'Sensor speichern',
            saveCustomSensor: 'Benutzerdefinierten Sensor speichern',
            unsavedChanges:
                'Es gibt ungespeicherte Änderungen an diesem benutzerdefinierten Sensor.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
            stay: 'Bleiben',
            discard: 'Verwerfen',
            selectCustomSensorFile: 'Benutzerdefinierte Sensordatei auswählen',
            deleteCustomSensor: 'Benutzerdefinierten Sensor löschen',
            deleteCustomSensorConfirm:
                'Sind Sie sicher, dass Sie den benutzerdefinierten Sensor löschen möchten: "{name}"?',
        },
        dashboard: {
            timeRange: 'Zeitbereich',
            minutes: 'min',
            chartType: 'Diagrammtyp',
            dataType: 'Datentyp',
            filterSensors: 'Sensoren filtern',
            showControls: 'Steuerungen anzeigen',
            mouseActions:
                'Dashboard-Mausaktionen:\n- Markieren zum Zoomen.\n- Scrollen zum Zoomen.\n- Rechtsklick zum Schwenken im gezoomten Zustand.\n- Doppelklick zum Zurücksetzen und fortsetzen der Aktualisierung.\n- Strg+Klick oder Mausklick in der Mitte, um alle Sensoren im Tooltip anzuzeigen.',
            fullPage: 'Vollseite',
            filterBySensor: 'Nach Sensor filtern',
            search: 'Suchen',
            filterTypes: 'Typen filtern',
            filterByDataType: 'Nach Datentyp filtern',
            selectChartType: 'Einen Diagrammtyp auswählen',
            exitFullPage: 'Vollseite verlassen',
            controls: 'Steuerungen',
            sensorValues: 'Sensorwerte',
            selected: 'Ausgewählt',
            clear: 'Löschen',
            deleteDashboard: 'Dashboard löschen',
            deleteDashboardConfirm:
                'Sind Sie sicher, dass Sie das Dashboard löschen möchten: "{name}"?',
            dashboardDeleted: 'Dashboard gelöscht',
            setAsHome: 'Als Startseite festlegen',
            duplicateDashboard: 'Dashboard duplizieren',
        },
        appInfo: {
            title: 'Anwendungsinformationen',
            noWarranty: 'Dieses Programm kommt absolut ohne Garantie.',
            daemonStatus: 'Daemon-Status',
            acknowledgeIssues: 'Probleme bestätigen',
            status: 'Status',
            processStatus: 'Prozessstatus',
            host: 'Host',
            uptime: 'Laufzeit',
            version: 'Version',
            processId: 'Prozess-ID',
            memoryUsage: 'Speichernutzung',
            liquidctl: 'Liquidctl',
            connected: 'Verbunden',
            disconnected: 'Nicht verbunden',
            helpfulLinks: 'Hilfreiche Links',
            gettingStarted: 'Erste Schritte',
            helpSettingUp: 'Hilfe bei der Einrichtung der Lüftersteuerung',
            hardwareSupport: 'Hardware-Unterstützung',
            hardwareSupportDesc: 'Unterstützte Geräte und Treiberinstallation',
            gitRepository: 'Git Repository',
            gitRepositoryDesc: 'Probleme melden oder Funktionen anfragen',
            discord: 'Discord',
            discordDesc: 'Treten Sie unserer Discord-Community bei',
            logsAndDiagnostics: 'Logs und Diagnose',
            downloadCurrentLog: 'Aktuelle Logs herunterladen',
        },
        alerts: {
            createAlert: 'Warnung erstellen',
            editAlert: 'Warnung bearbeiten',
            deleteAlert: 'Warnung löschen',
            noAlerts: 'Keine Warnungen konfiguriert',
            alertsOverview: 'Warnungsübersicht',
            alertLogs: 'Warnungsprotokolle',
            alertTriggered: 'Warnung ausgelöst',
            alertRecovered: 'Warnung wiederhergestellt',
            deleteAlertConfirm: 'Sind Sie sicher, dass Sie löschen möchten: "{name}"?',
            saveAlert: 'Warnung speichern',
            channelSource: 'Kanalquelle für Warnung',
            channelSourceTooltip: 'Die Kanalquelle, die für die Warnung verwendet werden soll',
            triggerConditions: 'Auslösebedingungen',
            maxValueTooltip: 'Werte über diesem lösen die Warnung aus.',
            minValueTooltip: 'Werte unter diesem lösen die Warnung aus.',
            warmupDurationTooltip:
                'Gibt an, wie lange eine Bedingung aktiv sein muss, bevor die Warnung als aktiv gilt. Die Überprüfung erfolgt nur in regelmäßigen Poll-Intervallen und kann daher von dieser Länge abweichen.',
            greaterThan: 'größer als',
            lessThan: 'kleiner als',
            newAlert: 'Neue Warnung',
            warmupGreaterThan: 'bedingung ausgelöst länger als',
            unsavedChanges: 'Es gibt ungespeicherte Änderungen an dieser Warnung.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
        },
        profiles: {
            createProfile: 'Profil erstellen',
            editProfile: 'Profil bearbeiten',
            deleteProfile: 'Profil löschen',
            noProfiles: 'Keine Profile konfiguriert',
            systemDefault: 'Systemstandard',
            profileType: 'Profiltyp',
            fixedDuty: 'Feste Lüftergeschwindigkeit',
            selectedPointDuty: 'Ausgewählter Punkt Leistung',
            selectedPointTemp: 'Ausgewählter Punkt Temperatur',
            tempSource: 'Temperaturquelle',
            memberProfiles: 'Mitgliedsprofile',
            mixFunction: 'Mischfunktion',
            applyMixFunction: 'Mischfunktion auf ausgewählte Profile anwenden',
            profilesToMix: 'Profile zum Mischen',
            saveProfile: 'Profil speichern',
            function: 'Funktion',
            functionToApply: 'Anzuwendende Funktion',
            graphProfileMouseActions:
                'Grafikprofil Mausaktionen:\n- Scrollen zum Zoomen.\n- Linksklick auf Linie um Punkt hinzuzufügen.\n- Rechtsklick auf Punkt zum Entfernen.\n- Punkt ziehen zum Verschieben.',
            unsavedChanges: 'Es gibt ungespeicherte Änderungen an diesem Profil.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
            appliedFunction: 'Angewendete Funktion',
            newProfile: 'Neues Profil',
            tooltip: {
                profileType:
                    'Profiltyp:<br/>- Standard: Behält aktuelle Geräteeinstellungen bei<br/>&nbsp;&nbsp;(BIOS/Firmware)<br/>- Fest: Stellt eine konstante Geschwindigkeit ein<br/>- Grafik: Anpassbare Lüfterkurve<br/>- Mix: Kombiniert mehrere Profile',
            },
            profileDeleted: 'Profil gelöscht',
            profileDuplicated: 'Profil dupliziert',
            deleteProfileConfirm: 'Sind Sie sicher, dass Sie löschen möchten: "{name}"?',
            deleteProfileWithChannelsConfirm:
                '"{name}" wird derzeit verwendet von: {channels}.\nDas Löschen dieses Profils wird die Einstellungen dieser Kanäle zurücksetzen.\nSind Sie sicher, dass Sie "{name}" löschen möchten?',
            profileUpdated: 'Profil erfolgreich aktualisiert',
            profileUpdateError:
                'Bei dem Versuch, dieses Profil zu aktualisieren, ist ein Fehler aufgetreten',
            tempSourceRequired: 'Für ein Grafikprofil ist eine Temperaturquelle erforderlich.',
            memberProfilesRequired:
                'Für ein Mischprofil sind mindestens 2 Mitgliedsprofile erforderlich.',
        },
        controls: {
            viewType: 'Ansichtstyp',
            controlOrView: 'Steuern oder Anzeigen',
        },
        modes: {
            createMode: 'Modus erstellen',
            editMode: 'Modus bearbeiten',
            deleteMode: 'Modus löschen',
            noModes: 'Keine Modi konfiguriert',
            deleteModeConfirm: 'Sind Sie sicher, dass Sie den Modus löschen möchten: "{name}"?',
            updateModeConfirm:
                'Sind Sie sicher, dass Sie "{name}" mit der aktuellen Konfiguration überschreiben möchten?',
            duplicateMode: 'Modus duplizieren',
        },
        functions: {
            createFunction: 'Funktion erstellen',
            editFunction: 'Funktion bearbeiten',
            deleteFunction: 'Funktion löschen',
            noFunctions: 'Keine Funktionen konfiguriert',
            saveFunction: 'Funktion speichern',
            functionType: 'Funktionstyp',
            functionTypeTooltip:
                'Funktionstypen:<br/>- Identität: Verändert den berechneten Profilwert nicht.<br/>- Standard: Verändert den Profilwert mithilfe eines Algorithmus mit Hysterese-Einstellungen.<br/>- Exponentieller gleitender Durchschnitt: Verändert den Profilwert mithilfe eines EMA-Algorithmus.',
            minimumAdjustment: 'Minimale Anpassung',
            minimumAdjustmentTooltip:
                'Minimale Lüftergeschwindigkeitsanpassung: Berechnete Änderungen unter diesem Wert werden ignoriert.',
            maximumAdjustment: 'Maximale Anpassung',
            maximumAdjustmentTooltip:
                'Maximale Lüftergeschwindigkeitsanpassung: Berechnete Änderungen über diesem Schwellenwert werden begrenzt.',
            windowSize: 'Fenstergröße',
            windowSizeTooltip:
                'Passen Sie die Empfindlichkeit gegenüber Temperaturänderungen an, indem Sie die Fenstergröße einstellen.\nKleinere Fenstergrößen reagieren schnell auf Änderungen,\nwährend größere Fenstergrößen gleichmäßigere Durchschnitte liefern.',
            hysteresisThreshold: 'Hysterese-Schwellenwert',
            hysteresisThresholdTooltip:
                'Temperaturänderungsschwellenwert (°C): Passen Sie die Lüftergeschwindigkeit an, wenn sich die Temperatur um diesen Betrag ändert.',
            hysteresisDelay: 'Hysterese-Verzögerung',
            hysteresisDelayTooltip:
                'Zeit (Sekunden), die benötigt wird, um auf Temperaturänderungen zu reagieren.',
            onlyDownward: 'Nur abwärts',
            onlyDownwardTooltip: 'Einstellungen nur anwenden, wenn die Temperatur sinkt.',
            unsavedChanges: 'Es gibt ungespeicherte Änderungen an dieser Funktion.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
            functionError: 'Fehler beim Versuch, diese Funktion zu aktualisieren',
            newFunction: 'Neue Funktion',
            functionDeleted: 'Funktion gelöscht',
            functionDuplicated: 'Funktion dupliziert',
            deleteFunctionConfirm: 'Sind Sie sicher, dass Sie "{name}" löschen möchten?',
            deleteFunctionWithProfilesConfirm:
                '"{name}" wird derzeit von den Profilen verwendet: {profiles}.\nDas Löschen dieser Funktion wird die Funktionen dieser Profile zurücksetzen.\nSind Sie sicher, dass Sie "{name}" löschen möchten?',
            functionUpdated: 'Funktion aktualisiert',
            functionUpdateError:
                'Beim Versuch, diese Funktion zu aktualisieren, ist ein Fehler aufgetreten',
        },
        error: {
            connectionError: 'CoolerControl-Verbindungsfehler',
            connectionToast: 'Verbindung zum Daemon nicht möglich',
            connectionToastDetail:
                'Verbindung zum Daemon nicht möglich. Bitte stellen Sie sicher, dass der Dienst läuft und versuchen Sie, erneut zu verbinden.',
            connectionRetryFailure:
                'Verbindung fehlgeschlagen - Wiederholungsversuch fehlgeschlagen',
            connectionRetryDetail:
                'Konnte nach mehreren Versuchen keine Verbindung zum Daemon herstellen.',
            errorLoadingGraph: 'Fehler beim Laden des Diagramms',
            highCpuUsageWarning: 'Hohe CPU-Auslastung erkannt',
            highCpuUsageDetail:
                'Die aktuelle CPU-Auslastung ist hoch.\nUm die Systembelastung zu reduzieren, erwägen Sie:\n1. Die Anzahl der angezeigten Diagramme zu reduzieren\n2. Die Anzahl der überwachten Sensoren zu reduzieren\n3. Das Abfrageintervall zu erhöhen',
            pageNotFound: 'Seite nicht gefunden',
            returnToDashboard: 'Zurück zum Dashboard',
            connectionErrorMessage: 'Konnte keine Verbindung zum CoolerControl-Daemon herstellen.',
            serviceRunningMessage: 'Bitte überprüfen Sie, ob der Daemon-Dienst läuft.',
            checkProjectPage: 'Für Hilfe bei der Einrichtung des Daemons, siehe die',
            projectPage: 'Projektseite',
            helpfulCommands: 'Hilfreiche Befehle:',
            nonStandardAddress:
                'Wenn Sie eine nicht standardmäßige Daemon-Adresse haben, können Sie sie unten angeben:',
            daemonAddressDesktop: 'Daemon-Adresse (Desktop-App)',
            daemonAddressWeb: 'Daemon-Adresse (Web-UI)',
            addressTooltip:
                'Die IP-Adresse oder der Domainname, mit dem eine Verbindung hergestellt werden soll.',
            portTooltip: 'Der Port, mit dem eine Verbindung hergestellt werden soll.',
            sslTooltip: 'Ob eine Verbindung zum Daemon über SSL/TLS hergestellt werden soll.',
            saveTooltip: 'Einstellungen speichern und die UI neu laden',
            resetTooltip: 'Auf Standardeinstellungen zurücksetzen',
        },
        singleDashboard: {
            minutes: 'min',
            chartMouseActions:
                'Dashboard-Mausaktionen:\n- Markieren zum Zoomen.\n- Scrollen zum Zoomen.\n- Rechtsklick zum Schwenken im gezoomten Zustand.\n- Doppelklick zum Zurücksetzen und fortsetzen der Aktualisierung.',
            timeRange: 'Zeitbereich',
            chartType: 'Diagrammtyp',
        },
        mode: {
            activateMode: 'Modus aktivieren',
            currentlyActive: 'Derzeit aktiv',
            modeHint:
                'Hinweis: Modi enthalten keine Profil- oder Funktionseinstellungen, nur Kanalkonfigurationen.',
        },
        lighting: {
            saveLightingSettings: 'Beleuchtungseinstellungen speichern',
            lightingMode: 'Beleuchtungsmodus',
            speed: 'Geschwindigkeit',
            direction: 'Richtung',
            forward: 'Vorwärts',
            backward: 'Rückwärts',
            numberOfColors: 'Anzahl der Farben',
            numberOfColorsTooltip:
                'Anzahl der Farben, die für den ausgewählten Beleuchtungsmodus verwendet werden sollen.',
        },
        lcd: {
            saveLcdSettings: 'LCD-Einstellungen speichern',
            lcdMode: 'LCD-Modus',
            brightness: 'Helligkeit',
            brightnessPercent: 'Helligkeit in Prozent',
            orientation: 'Ausrichtung',
            orientationDegrees: 'Ausrichtung in Grad',
            chooseImage: 'Bild auswählen',
            dragAndDrop: 'Dateien hierher ziehen und ablegen.',
            tempSource: 'Temp-Quelle',
            tempSourceTooltip: 'Temperaturquelle, die in der LCD-Anzeige verwendet werden soll.',
            imagesPath: 'Bilder-Pfad',
            imagesPathTooltip:
                'Geben Sie den absoluten Pfad zum Verzeichnis mit den Bildern ein.\nDas Verzeichnis muss mindestens eine Bilddatei enthalten, und diese\nkönnen statische Bilder oder GIFs sein. Das Karussell wird\nsie mit der ausgewählten Verzögerung durchlaufen. Alle Dateien werden\nbei der Übermittlung verarbeitet, um maximale Kompatibilität zu gewährleisten.',
            browse: 'Durchsuchen',
            browseTooltip: 'Nach einem Bildverzeichnis suchen',
            delayInterval: 'Verzögerungsintervall',
            delayIntervalTooltip:
                'Minimale Anzahl der Sekunden Verzögerung zwischen Bildwechseln.\nBeachten Sie, dass die tatsächliche Verzögerung aufgrund der Daemon-Abfragerate länger sein kann.',
            processing: 'Verarbeitung...',
            applying: 'Anwenden...',
            unsavedChanges: 'Es gibt ungespeicherte Änderungen an diesen LCD-Einstellungen.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
            imageTooLarge: 'Bild ist zu groß. Bitte wählen Sie ein kleineres.',
            notImageType: 'Bild wird nicht als Bildtyp erkannt',
        },
    },
    daemon: {
        status: {
            ok: 'Ok',
            hasWarnings: 'Hat Warnungen',
            hasErrors: 'Hat Fehler',
        },
    },
    components: {
        confirmation: {
            title: 'Bestätigung',
            message: 'Sind Sie sicher?',
        },
        aseTek690: {
            sameDeviceID:
                'Die Legacy-NZXT-Krakens und die EVGA CLC haben zufällig dieselbe Geräte-ID, und CoolerControl kann nicht feststellen, welches Gerät angeschlossen ist. Dies ist für die ordnungsgemäße Gerätekommunikation erforderlich.',
            restartRequired:
                'Ein Neustart der CoolerControl systemd-Dienste kann erforderlich sein und wird bei Bedarf automatisch durchgeführt.',
            deviceModel: 'Ist das Liquidctl-Gerät eines der folgenden Modelle?',
            modelList: 'NZXT Kraken X40, X60, X31, X41, X51 oder X61',
            acceptLabel: 'Ja, es ist ein Legacy-Kraken-Gerät',
            rejectLabel: 'Nein, es ist ein EVGA CLC-Gerät',
        },
        password: {
            title: 'Geben Sie Ihr Passwort ein',
            newPasswordTitle: 'Geben Sie ein neues Passwort ein',
            invalidPassword: 'Ungültiges Passwort',
            passwordHelp:
                'Bei der Installation verwendet der Daemon ein Standardpasswort zum Schutz der Gerätesteuerungsendpunkte. \nOptional können Sie ein starkes Passwort für verbesserten Schutz erstellen. \nWenn Sie diesen Dialog sehen und noch kein Passwort festgelegt haben, versuchen Sie, die UI zu aktualisieren \n oder auf Anmelden im Zugriffsschutzmenu zu klicken. Weitere Informationen finden Sie im Projekt-Wiki.',
        },
        notFound: {
            message: 'Genau wie die perfekte Linux 🐧 Distribution\nexistiert diese Seite nicht.',
        },
        helloWorld: {
            message:
                'Sie haben erfolgreich ein Projekt mit Vite + Vue 3 erstellt. Was kommt als nächstes?',
        },
        dashboardInfo: {
            description:
                'Dashboards ermöglichen es Ihnen, die Sensordaten Ihres Systems gemäß Ihren Vorlieben anzuzeigen. Sie können zwischen zeitbasierten oder tabellenbasierten Diagrammen wählen und die Filter und Einstellungen für jedes Diagramm anpassen, um sich auf die spezifischen Daten zu konzentrieren, die Sie anzeigen möchten. Zusätzlich können Sie mehrere Dashboards erstellen, die an Ihre Bedürfnisse angepasst sind.',
        },
        modeInfo: {
            description:
                'Modi ermöglichen es Ihnen, Gerätekanaleinstellungen für eine schnelle und einfache Anwendung zu speichern. Zum Beispiel können Sie einen "Spiel"-Modus und einen "Leise"-Modus erstellen, sodass Sie leicht zwischen ihnen wechseln können.',
            note: 'Bitte beachten Sie, dass es möglicherweise notwendig ist, verschiedene Lüfterprofile für jeden Modus zu erstellen, da Modi nur Kanalkonfigurationen enthalten und keine internen Profil- oder Funktionseinstellungen umfassen.',
        },
        alertInfo: {
            description:
                'Warnungen werden verwendet, um Sie zu benachrichtigen, wenn bestimmte Bedingungen auftreten. Sie können Temperaturen und Lüftergeschwindigkeiten überwachen, um sicherzustellen, dass Ihr System ordnungsgemäß läuft. Warnungen werden für bestimmte Sensorwertbereiche konfiguriert und senden Benachrichtigungen, wenn Werte überschritten werden oder in akzeptable Schwellenwertbereiche zurückkehren.',
        },
        customSensorInfo: {
            title: 'Benutzerdefinierte Sensor-Übersicht',
            description:
                'Benutzerdefinierte Sensoren ermöglichen es Ihnen, bestehende Sensoren auf verschiedene Weise zu kombinieren und verbessern Ihre Kontrolle und Effizienz bei der Systemkühlung. Zusätzlich unterstützen sie dateibasierte Daten, die es Ihnen ermöglichen, externe Sensoreingaben für größere Flexibilität zu skripten.',
            note: 'Hinweis: Sie können Mix-Profile verwenden, um mehrere benutzerdefinierte Sensorausgaben zu kombinieren.',
        },
        functionInfo: {
            title: 'Funktionsübersicht',
            description:
                'Funktionen sind konfigurierbare Algorithmen, die auf Profilausgaben angewendet werden. Sie ermöglichen es Ihnen, zu verwalten, wann Lüftergeschwindigkeitsänderungen auftreten, Hysterese-Einstellungen anzupassen und gleitende Durchschnitte für dynamische Temperaturen zu verwenden.',
            identityFunction:
                'Die Identitätsfunktion ist die einfachste Option, da sie die berechnete Profilausgabe nicht modifiziert; sie erlaubt es Ihnen nur, minimale und maximale Geschwindigkeitsänderungsbereiche festzulegen. Dies ist besonders vorteilhaft, um ständige Lüftergeschwindigkeitsschwankungen zu minimieren.',
        },
        profileInfo: {
            title: 'Profilübersicht',
            description:
                'Profile definieren anpassbare Einstellungen zur Steuerung der Lüftergeschwindigkeiten, wobei dasselbe Profil für mehrere Lüfter verwendet werden kann. Typen umfassen:',
            type: {
                fixed: 'Feste Geschwindigkeit',
                fanCurve: 'Lüfterkurve/Graph',
                mix: 'Mix-Profil',
                default: 'Standard-Geräteeinstellungen',
            },
            additionalInfo:
                'Profile sind die Grundlage für die Steuerung der Lüftergeschwindigkeiten und können durch Anwendung fortschrittlicherer Algorithmusfunktionen weiter verbessert werden.',
        },
        deviceInfo: {
            details: 'Gerätedetails',
            systemName: 'Systemname',
            deviceType: 'Gerätetyp',
            deviceUID: 'Geräte-UID',
            firmwareVersion: 'Firmware-Version',
            model: 'Modell',
            driverName: 'Treibername',
            driverType: 'Treibertyp',
            driverVersion: 'Treiberversion',
            locations: 'Standorte',
        },
        onboarding: {
            welcome: 'Willkommen bei CoolerControl!',
            beforeStart:
                'Bevor wir beginnen, ist eine der wichtigsten Dinge, die Sie wissen sollten,',
            settingUpDrivers: 'das Einrichten Ihrer Hardware-Treiber',
            fansNotShowing:
                'Wenn Ihre Lüfter nicht angezeigt werden oder nicht gesteuert werden können, dann gibt es wahrscheinlich ein Problem mit Ihren derzeit installierten Kernel-Treibern.',
            checkDocs:
                'Bevor Sie ein Problem melden, stellen Sie bitte sicher, dass alle Treiber ordnungsgemäß geladen wurden, indem Sie',
            checkingDocs: 'die Hardware-Support-Dokumentation überprüfen',
            startTourAgain:
                'Hinweis: Sie können diese Tour jederzeit über die Einstellungsseite erneut starten.',
            letsStart: 'Ok, lass uns beginnen!',
            dashboards: 'Dashboards',
            dashboardsDesc:
                'Dashboards sind eine kuratierte Sammlung von Diagrammen zur Ansicht der Sensordaten Ihres Systems.',
            controls: 'Steuerungen',
            controlsDesc:
                'Steuerungen bieten eine interaktive Oberfläche zur Verwaltung der Lüfter und anderer Geräte Ihres Systems. Jeder steuerbare Kanal ermöglicht es Ihnen, die Geschwindigkeiten anzupassen, Profile festzulegen und den Status in Echtzeit zu überwachen.',
            profiles: 'Profile',
            profilesDesc:
                'Profile definieren anpassbare Einstellungen zur Steuerung der Lüftergeschwindigkeiten. Dasselbe Profil kann für mehrere Lüfter und Geräte verwendet werden.',
            functions: 'Funktionen',
            functionsDesc:
                'Funktionen sind konfigurierbare Algorithmen, die auf die Ausgabe eines Profils angewendet werden können. Dies kann hilfreich sein, um zu verwalten, wann Lüftergeschwindigkeitsänderungen auftreten.',
            appInfo: 'Anwendungs- und Daemon-Informationen',
            appInfoDesc:
                'Durch Klicken auf das Logo wird die Anwendungsinformationsseite geöffnet, wo Sie Informationen über die Anwendung, den Systemdaemon und Logs erhalten können. Hier sollten Sie bei der Fehlerbehebung nachsehen, und es gibt ein kleines Daemon-Status-Symbol, das Sie über potenzielle Probleme informiert.',
            quickAdd: 'Schnelles Hinzufügen',
            quickAddDesc:
                'Dies ist ein Menü, um neue Elemente wie Dashboards, Profile usw. einfach hinzuzufügen.',
            dashboardQuick: 'Dashboard-Schnellmenü',
            dashboardQuickDesc:
                'Dies ist ein Menü, um schnell zu Ihren Dashboards zu springen, auch wenn das Hauptmenü eingeklappt ist.',
            settings: 'Einstellungen',
            settingsDesc:
                'Diese Schaltfläche öffnet die Einstellungsseite mit verschiedenen UI- und Daemon-Einstellungen.',
            restartMenu: 'Neustart-Menü',
            restartMenuDesc:
                'Hier können Sie wählen, ob Sie die UI neu laden oder den Systemdaemon neu starten möchten.',
            thatsIt: "Das war's!",
            ready: 'Und denken Sie daran, wenn Ihre Lüfter nicht angezeigt werden oder nicht gesteuert werden können, überprüfen Sie die Hardware-Support-Dokumentation',
            startNow: 'Ok, Sie können jetzt loslegen!',
        },
        axisOptions: {
            title: 'Achsenoptionen',
            autoScale: 'Automatische Skalierung',
            max: 'Max',
            min: 'Min',
            dutyTemperature: 'Auslastung / Temperatur',
            rpmMhz: 'U/min / MHz',
            krpmGhz: 'kU/min / GHz',
            watts: 'Watt',
        },
        sensorTable: {
            device: 'Gerät',
            channel: 'Kanal',
            current: 'Aktuell',
            min: 'Min',
            max: 'Max',
            average: 'Durchschnitt',
        },
        modeTable: {
            setting: 'Einstellung',
        },
        wizards: {
            fanControl: {
                fanControlWizard: 'Fan Control Wizard',
                editCurrentProfile: 'Aktuelles Profil bearbeiten',
                editCurrentFunction: 'Aktuelle Funktion bearbeiten',
                currentSettings: 'Aktuelle Einstellungen anzeigen',
                manualSpeed: 'Manuelle Lüftergeschwindigkeit einstellen',
                createNewProfile: 'Erstelle ein neues Profil',
                existingProfile: 'Wählen Sie ein vorhandenes Profil',
                resetSettings: 'Zurücksetzen auf die Werkseinstellungen',
                chooseProfileNameType: 'Wählen Sie einen Profilnamen und einen Typ',
                newDefaultProfile: 'Neues Standardprofil',
                profileCreatedApplied: 'Profil erstellt und angewendet',
                willCreatedAndAppliedTo: 'wird erstellt und angewendet auf',
                newFixedProfile: 'Neues festes Profil',
                withSettings: 'mit den folgenden Einstellungen',
                selectSpeed: 'Wählen Sie Ihre Geschwindigkeit',
                newMixProfile: 'Neues Mix-Profil',
                newGraphProfile: 'Neues Graph-Profil',
                functionFor: 'Wählen Sie eine anzuwendende Funktion aus',
                functionDescription:
                    'Funktionen ermöglichen es Ihnen, weiter zu steuern, wie das Profil-Output angewendet wird.',
                createNewFunction: 'Erstelle eine neue Funktion',
                existingFunction: 'Wählen Sie eine vorhandene Funktion',
                defaultFunction: 'Verwenden Sie die Standardfunktion',
                chooseFunctionNameType: 'Wählen Sie einen Funktionsnamen und -typ',
                newFunctionName: 'Funktion für {profileName}',
                summary: 'Zusammenfassung',
                aNewProfile: 'Ein neues Profil',
                andFunction: 'und Funktion',
            },
            profile: {
                willCreated: 'wird erstellt.',
            },
            customSensor: {
                new: 'Neuer benutzerdefinierter Sensor',
            },
        },
    },
    auth: {
        enterPassword: 'Geben Sie Ihr Passwort ein',
        setNewPassword: 'Geben Sie ein neues Passwort ein',
        loginFailed: 'Anmeldung fehlgeschlagen',
        invalidPassword: 'Ungültiges Passwort',
        passwordSetFailed: 'Passwort setzen fehlgeschlagen',
        passwordSetSuccessfully: 'Neues Passwort erfolgreich gesetzt',
        logoutSuccessful: 'Sie haben sich erfolgreich abgemeldet.',
        unauthorizedAction: 'Sie müssen angemeldet sein, um diese Aktion abzuschließen',
    },
    device_store: {
        unauthorized: {
            summary: 'Nicht autorisiert',
            detail: 'Sie müssen angemeldet sein, um diese Aktion abzuschließen',
        },
        login: {
            success: {
                summary: 'Erfolg',
                detail: 'Anmeldung erfolgreich.',
            },
            failed: {
                summary: 'Anmeldung fehlgeschlagen',
                detail: 'Ungültiges Passwort',
            },
        },
        logout: {
            summary: 'Abmeldung',
            detail: 'Sie haben sich erfolgreich abgemeldet.',
        },
        password: {
            set_success: {
                summary: 'Passwort',
                detail: 'Neues Passwort erfolgreich gesetzt',
            },
            set_failed: {
                summary: 'Passwort setzen fehlgeschlagen',
            },
        },
        asetek: {
            header: 'Unbekanntes Gerät erkannt',
            success: {
                summary: 'Erfolg',
                detail_legacy:
                    'Gerätemodelltyp erfolgreich festgelegt. Neustart wird durchgeführt.',
                detail_evga: 'Gerätemodelltyp erfolgreich festgelegt.',
            },
            error: {
                summary: 'Fehler',
                detail: 'Prozess unterbrochen.',
            },
        },
    },
    models: {
        chartType: {
            timeChart: 'Zeitdiagramm',
            table: 'Tabelle',
            controls: 'Steuerungen',
        },
        dataType: {
            temp: 'Temp',
            duty: 'Auslastung',
            load: 'Last',
            rpm: 'U/min',
            freq: 'Freq',
            watts: 'Watt',
        },
        profile: {
            profileType: {
                default: 'Standard',
                fixed: 'Fest',
                graph: 'Graph',
                mix: 'Mix',
            },
            functionType: {
                identity: 'Identität',
                standard: 'Standard',
                exponentialMovingAvg: 'Exponentieller gleitender Durchschnitt',
            },
            mixFunctionType: {
                min: 'Minimum',
                max: 'Maximum',
                avg: 'Durchschnitt',
            },
        },
        customSensor: {
            sensorType: {
                mix: 'Mix',
                file: 'Datei',
            },
            mixFunctionType: {
                min: 'Minimum',
                max: 'Maximum',
                delta: 'Delta',
                avg: 'Durchschnitt',
                weightedAvg: 'Gewichteter Durchschnitt',
            },
        },
        themeMode: {
            system: 'System',
            dark: 'Dunkel',
            light: 'Hell',
            highContrastDark: 'Hoher Kontrast Dunkel',
            highContrastLight: 'Hoher Kontrast Hell',
            custom: 'Benutzerdefiniertes Theme',
        },
        channelViewType: {
            control: 'Steuerung',
            dashboard: 'Dashboard',
        },
        alertState: {
            active: 'Aktiv',
            inactive: 'Inaktiv',
            error: 'Fehler',
        },
        deviceType: {
            customSensors: 'Benutzerdefinierte Sensoren',
            cpu: 'CPU',
            gpu: 'GPU',
            liquidctl: 'Liquidctl',
            hwmon: 'Hwmon',
        },
        driverType: {
            kernel: 'Kernel',
            liquidctl: 'Liquidctl',
            nvml: 'NVML',
            nvidiaCli: 'Nvidia CLI',
            coolercontrol: 'CoolerControl',
        },
        lcdModeType: {
            none: 'Keine',
            liquidctl: 'Liquidctl',
            custom: 'Benutzerdefiniert',
        },
    },
}

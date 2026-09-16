// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

export default {
    common: {
        save: 'Speichern',
        mouseActions: 'Mausaktionen',
        moreInfo: 'Weitere Informationen',
        cancel: 'Abbrechen',
        add: 'Hinzufügen',
        yes: 'Ja',
        no: 'Nein',
        ok: 'OK',
        error: 'Fehler',
        success: 'Erfolg',
        loading: 'Lade...',
        restarting: 'Neustart...',
        retry: 'Wiederholen',
        saveAndRefresh: 'Speichern und aktualisieren',
        reset: 'Zurücksetzen',
        sslTls: 'SSL/TLS',
        protocol: 'Protokoll',
        address: 'Adresse',
        port: 'Port',
        search: 'Suchen',
        finish: 'Fertigstellen',
        next: 'Weiter',
        previous: 'Zurück',
        unmanaged: 'Ungesteuert',
        password: 'Passwort',
        currentPassword: 'Aktuelles Passwort',
        newPassword: 'Neues Passwort',
        confirmPassword: 'Passwort bestätigen',
        savePassword: 'Passwort speichern',
        state: 'Status',
        name: 'Name',
        message: 'Nachricht',
        timestamp: 'Zeitstempel',
        temperature: 'Temp.',
        duty: 'Leistung',
        offset: 'Versatz',
        stay: 'Bleiben',
        discard: 'Verwerfen',
        copy: '(Kopie)',
        minuteAbbr: 'Min',
        rpmAbbr: 'U/min',
        mhzAbbr: 'MHz',
        ghzAbbr: 'GHz',
        tempUnit: '°C',
        percentUnit: '%',
        secondAbbr: 's',
        wattAbbr: 'W',
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
        shell: {
            search: {
                hint: 'Geräte, Sensoren, Einstellungen und Aktionen suchen',
                recent: 'Zuletzt verwendet',
                jumpTo: 'Springen zu',
                noResults: 'Keine Treffer gefunden.',
                more: '{count} weitere',
                kindFan: 'Lüfter',
                kindSensor: 'Sensoren',
                kindAction: 'Aktionen',
                kindPage: 'Seiten',
            },
            home: 'Start',
            cooling: 'Kühlung',
            monitoring: 'Überwachung',
            devices: 'Geräte',
            settings: 'Einstellungen',
            plugins: 'Plugins',
            modes: 'Modi',
            manageModes: 'Modi verwalten',
            access: 'Zugriff',
            power: 'Ein/Aus',
            noModes: 'Keine Modi gespeichert',
            supportWizards: {
                summary: 'Support-Zauberer aktiviert!',
                detail: 'Mit Dank an die Freiwilligen, die unseren Nutzern helfen, Hardware und Treiber zum Laufen zu bringen.',
            },
            coolingPanel: {
                pinned: 'Angeheftet',
                pin: 'Anheften',
                unpin: 'Lösen',
                library: 'Profile & Funktionen',
                profiles: 'Profile',
                functions: 'Funktionen',
                addFolder: 'Ordner hinzufügen',
                newFolder: 'Neuer Ordner',
                deleteFolder: 'Ordner löschen',
            },
            monitoringPanel: {
                newDashboard: 'Neues Dashboard',
                createAlert: 'Warnung für diesen Sensor erstellen',
                failAlert: 'Ausfallwarnung erstellen (löst bei 0 U/min aus)',
                failAlertSuffix: 'Ausfall',
            },
            devicesPanel: {
                disabled: 'Deaktiviert',
            },
            sensorDest: {
                monitoring: 'Überwachung',
                cooling: 'Kühlung',
                lighting: 'Beleuchtung',
                lcd: 'LCD',
            },
            manageSensors: {
                title: 'Geräte und Sensoren verwalten',
                hint: 'Geräte und Sensoren aktivieren oder deaktivieren. Das Deaktivieren ungenutzter wird empfohlen.',
                pendingChanges: 'Keine Änderungen | {count} Änderung | {count} Änderungen',
                applyRestart: 'Anwenden & Neustart',
                disabledDevices: 'Deaktivierte Geräte',
                openButton: 'Geräte und Sensoren verwalten',
            },
            toast: {
                copy: 'Kopieren',
                dismissAll: 'Alle schließen',
            },
            homePanel: {
                overview: 'Übersicht',
                logs: 'Logs',
            },
            homePage: {
                viewLogs: 'Logs anzeigen',
                logsAll: 'Alle',
                logsWarnings: 'Warnungen+',
                logsErrors: 'Fehler',
                logsNoMatches: 'Keine passenden Log-Zeilen.',
                getStartedGroup: 'Erste Schritte',
                learnGroup: 'Lernen',
                resourcesGroup: 'Ressourcen',
                modeAndAlerts: 'Modus & Warnungen',
                noActiveMode: 'Kein aktiver Modus',
                setUpCooling: 'Kühlung einrichten',
            },
            devicesPage: {
                landingHint: 'Wählen Sie ein Gerät, um seine Details und Einstellungen anzuzeigen.',
                temps: 'Temp.',
                fans: 'Lüfter',
                lighting: 'Beleuchtung',
                lcd: 'LCD',
                deviceDisabled: 'Dieses Gerät ist deaktiviert.',
                enableDevice: 'Gerät aktivieren',
                disableUnusedSensors: 'Ungenutzte Sensoren deaktivieren… (empfohlen)',
                sensors: 'Sensoren',
            },
            hardwareHelp: {
                missingDevice: 'Hardware erwartet, die hier nicht aufgeführt ist?',
            },
            coolingPage: {
                landingHint:
                    'Wählen Sie einen Lüfter oder eine Pumpe, um dessen Kühlung anzuzeigen und anzupassen.',
                noChannels: 'Keine Lüfter- oder Pumpenkanäle erkannt.',
                noneControllable:
                    'Keiner der erkannten Lüfter- oder Pumpenkanäle kann gesteuert werden.',
                noticeBlockedByEnvironment:
                    'Die Hardware-Erkennung konnte nicht ausgeführt werden, daher fehlen möglicherweise Lüfter- und Pumpenkanäle.',
                fullChart: 'Vollständiges Diagramm',
                guidedSetup: 'Geführte Einrichtung',
                setupMenu: {
                    autoCreateThisFan: 'Für diesen Lüfter automatisch erstellen',
                    createProfile: 'Neues Profil erstellen',
                    calibrateThisFan: 'Diesen Lüfter kalibrieren',
                    autoCreateAllFans: 'Für alle Lüfter automatisch erstellen',
                    calibrateAllFans: 'Alle Lüfter kalibrieren',
                },
                manualAt: 'Manuell {duty}%',
                manualDuty: 'Manuelle Auslastung',
                modeProfile: 'Profil',
                modeManual: 'Manuell',
                modeUnmanaged: 'Ungesteuert',
                unmanagedHint:
                    'Das Gerät oder seine Firmware steuert diesen Kanal. CoolerControl sendet keine Drehzahlbefehle.',
                apply: 'Anwenden',
                saveAndApply: 'Speichern & Anwenden',
                unsavedChanges: 'An diesem Kanal gibt es Änderungen, die nicht angewendet wurden.',
                unsavedChangesHeader: 'Nicht gespeicherte Änderungen',
                selectProfile: 'Profil auswählen',
                sharedWith: 'Geteilt mit {count} weiteren',
                sharedTooltip: 'Dieses Profil steuert auch andere Kanäle.',
                notShared: 'Nur dieser Lüfter',
                notSharedTooltip: 'Dieses Profil steuert nur diesen Kanal.',
                forkForFan: 'Für diesen Lüfter abspalten',
                forkQualifier: '{channel} Kopie',
                fork: {
                    confirmHeader: 'Für diesen Lüfter abspalten',
                    confirmMessage:
                        "Das Profil '{profile}' in ein neues Profil '{copy}' kopieren und {channel} zuweisen.\n\nDas Original bleibt unverändert, Änderungen hier betreffen also nur {channel}.",
                    accept: 'Kopie erstellen',
                },
                convert: {
                    button: 'Für Kalibrierung umrechnen',
                    tooltip:
                        'Dieser Lüfter ist kalibriert, daher werden seine gespeicherten Drehzahlen jetzt als echte Drehzahlen gelesen und bei jedem Schreibvorgang neu zugeordnet. Rechne sie um, damit der Lüfter sich wie vor der Kalibrierung verhält.',
                    confirmHeader: 'Für den kalibrierten Lüfter umrechnen',
                    confirmProfile:
                        "Das Profil '{profile}' in ein neues Profil '{copy}' kopieren, seine Drehzahlen umrechnen und {channel} zuweisen.\n\nRechne nur Drehzahlen um, die du vor der Kalibrierung dieses Lüfters gesetzt hast. Eine doppelte Umrechnung lässt den Lüfter mit falscher Drehzahl laufen. Das Original bleibt unverändert.",
                    confirmManual:
                        'Die manuelle Leistung für {channel} umrechnen, damit der Lüfter die Drehzahl von vor der Kalibrierung hält.\n\nRechne nur einen Wert um, den du vor der Kalibrierung dieses Lüfters gesetzt hast. Eine doppelte Umrechnung lässt den Lüfter mit falscher Drehzahl laufen.',
                    nameQualifier: 'kalibriert',
                    accept: 'Umrechnen',
                    successProfile:
                        "'{profile}' wurde {channel} mit umgerechneten Drehzahlen zugewiesen.",
                    successManual: 'Manuelle Leistung auf {duty}% umgerechnet.',
                    error: 'Die Drehzahlen für diesen Lüfter konnten nicht umgerechnet werden.',
                    floorHeading: 'Einige Punkte wurden zu 0%',
                    floorNotice:
                        '{count} Punkt(e) lagen unter der langsamsten Drehzahl, die für {channel} nach der Kalibrierung einstellbar ist, und wurden daher zu 0%. Prüfe die neue Kurve, bevor du dich darauf verlässt.',
                    modesHeading: 'Modi verwenden weiterhin das Original',
                    modesReminder:
                        'Diese Modi weisen {channel} weiterhin das ursprüngliche Profil zu: {modes}. Aktualisiere sie auf die umgerechnete Kopie.',
                },
                notControllable:
                    'Dieser Kanal meldet seine Drehzahl, kann aber von CoolerControl nicht gesteuert werden.',
                verdictFirmwareOverride:
                    'CoolerControl hat diesen Kanal auf manuelle Steuerung gesetzt, doch die Firmware hat das zurückgesetzt.',
                verdictFamilyMayNeedOutOfTree:
                    'Für diesen Kanal wurde keine schreibbare Lüftersteuerung gefunden. Bei dieser Chipfamilie stellt sie manchmal ein anderer Kernel-Treiber bereit.',
                verdictNotSupportedByDriver:
                    'Der verwendete Treiber stellt für diesen Kanal keine Lüftersteuerung bereit.',
                verdictNoPwm:
                    'Der geladene Treiber stellt für diesen Kanal keine Lüftersteuerung bereit, nur dessen Drehzahl.',
                verdictPwmReadOnly:
                    'Der geladene Treiber stellt für diesen Kanal eine Lüftersteuerung bereit, markiert sie aber als schreibgeschützt.',
                verdictIgnoresDuty:
                    'Dieser Kanal hat Leistungsänderungen angenommen, seine gemessene Drehzahl hat aber nie reagiert.',
                verdictUnverifiable:
                    'Dieser Kanal hat keinen nutzbaren Drehzahlmesser, daher lässt sich seine Reaktion auf Leistungsänderungen nicht überprüfen.',
                verdictEvidenceLabel: 'Auf diesem Rechner gemessen:',
                evidenceNoPwmFile: 'keine Lüftersteuerung vorhanden',
                evidencePwmNotWritable: 'Lüftersteuerung ist schreibgeschützt',
                evidenceHasTachometer: 'Drehzahlanzeige verfügbar',
                evidenceNoTachometer: 'keine Drehzahlanzeige',
                verdictLearnMore: 'Was kann ich dagegen tun?',
                verdictFoundSomethingThatWorks:
                    'Etwas gefunden, das funktioniert? Sagen Sie es uns',
                activeMode: 'Aktiv',
                previousMode: 'Vorheriger',
                activate: 'Aktivieren',
                noModes:
                    'Noch keine Modi gespeichert. Modi speichern alle Kanaleinstellungen für schnelles Umschalten.',
                powerProfiles: {
                    title: 'Systemenergieprofil',
                    description:
                        'Einen Modus automatisch aktivieren, wenn sich das Energieprofil des Systems aendert.',
                    activeProfile: 'Aktuelles Profil: {profile}',
                    noMode: 'Kein Modus',
                    saveFailed: 'Die Zuordnung der Energieprofile konnte nicht gespeichert werden.',
                    profileNames: {
                        'power-saver': 'Energiesparmodus',
                        balanced: 'Ausgeglichen',
                        performance: 'Leistung',
                    },
                },
                miniCurveHint:
                    'Kurve des zugewiesenen Profils. Der Punkt markiert das Ziel bei der aktuellen Temperatur der Quelle; die Funktion des Kanals formt den tatsächlichen Wert.',
                chain: {
                    tempSource: 'Temperaturquelle',
                    profile: 'Profil',
                    function: 'Funktion',
                },
            },
        },
        topbar: {
            login: 'Anmelden',
            logout: 'Abmelden',
            changePassword: 'Passwort ändern',
            accessTokens: 'Zugriffstoken',
            restartUI: 'UI neustarten',
            restartDaemonAndUI: 'Daemon und UI neustarten',
            restartConfirmMessage:
                'Sind Sie sicher, dass Sie den Daemon und die UI neustarten möchten?',
            restartConfirmHeader: 'Daemon Neustart',
            shutdownSuccess: 'Daemon-Shutdown-Signal akzeptiert',
            shutdownError:
                'Unbekannter Fehler beim Senden des Shutdown-Signals. Details finden Sie in den Logs.',
            quitDesktopApp: 'Desktop-App beenden',
            back: 'Zurück',
            expandMenu: 'Menü erweitern',
            collapseMenu: 'Menü einklappen',
            alerts: 'Warnungen',
            settings: 'Einstellungen',
            openInBrowser: 'Im Browser öffnen',
            loginSuccessful: 'Anmeldung erfolgreich',
        },
        settings: {
            title: 'Einstellungen',
            devices: {
                toggleRequiresRestart:
                    'Das Umschalten von Geräten oder Sensoren erfordert einen Neustart des Daemons und der UI. Möchten Sie das jetzt tun?',
                enableDevices: 'Geräte aktivieren',
                unknownError:
                    'Unbekannter Fehler beim Versuch, Änderungen auf alle Geräte anzuwenden. Details finden Sie in den Logs.',
            },
            plugins: {
                privileged: 'Privilegierter Zugriff',
                pluginUrl: 'Startseite',
                restricted: 'Eingeschränkter Zugriff',
                settingsSaved: 'Plugin-Einstellungen erfolgreich gespeichert',
                settingsNotSaved: 'Plugin-Einstellungen konnten nicht gespeichert werden',
            },
            appearance: 'Erscheinungsbild',
            general: 'Allgemein',
            language: 'Sprache',
            selectLanguage: 'Sprache auswählen',
            systemLanguage: 'System',
            fullScreen: 'Vollbild',
            railToCollapse: 'Leiste zum Einklappen',
            eyeCandy: 'Augenschmaus',
            interfaceFont: 'Schriftart der Oberfläche',
            introduction: 'Einführung',
            startTour: 'Tour starten',
            timeFormat: 'Zeitformat',
            time24h: '24 Stunden',
            time12h: '12 Stunden',
            frequencyPrecision: 'Frequenzgenauigkeit',
            startupPage: 'Startseite',
            dashboardLineSize: 'Dashboard-Liniengröße',
            themeStyle: 'Theme-Stil',
            themeGroups: {
                builtIn: 'Integriert',
                installed: 'Installiert',
                custom: 'Benutzerdefiniert',
            },
            desktop: 'Desktop',
            startInTray: 'Im Tray starten',
            closeToTray: 'In Tray minimieren',
            zoom: 'Zoom',
            desktopStartupDelay: 'Desktop-Startverzögerung',
            groups: {
                startup: 'Start',
                performance: 'Leistung',
                devices: 'Geräte & Erkennung',
                liquidctl: 'Liquidctl',
            },
            applySettingsOnStartup: 'Einstellungen beim Start anwenden',
            deviceDelayAtStartup: 'Geräteverzögerung beim Start',
            pollingRate: 'Abfragerate',
            compressApiPayload: 'API-Payload komprimieren',
            liquidctlIntegration: 'Liquidctl-Integration',
            liquidctlDeviceInit: 'Liquidctl-Geräteinitialisierung',
            hideDuplicateDevices: 'Doppelte Geräte ausblenden',
            drivePowerState: 'Festplattenstromzustand',
            sensorsAutoDetect: 'Sensoren automatisch erkennen',
            sensorsConfig: 'lm-sensors-Konfiguration',
            deviceListener: 'Geräteänderungs-Listener',
            customTheme: {
                title: 'Benutzerdefiniertes Theme',
                accent: 'Akzentfarbe',
                accentGradientTo: 'Akzent-Verlauf Ende',
                bgOne: 'Hintergrund Primär',
                bgTwo: 'Hintergrund Sekundär',
                border: 'Rahmenfarbe',
                text: 'Textfarbe',
                textSecondary: 'Sekundäre Textfarbe',
                success: 'Erfolg',
                warning: 'Warnung',
                error: 'Fehler',
                info: 'Info',
                export: 'Theme exportieren',
                import: 'Theme importieren',
                copyCode: 'Code kopieren',
                pasteCode: 'Code einfügen',
                themeCodeCopied: 'Theme-Code kopiert',
                themeApplied: 'Theme angewendet',
                invalidThemeCode: 'Ungültiger Theme-Code',
            },
            tooltips: {
                timeFormat: 'Zeitformat: 12-Stunden (AM/PM) oder 24-Stunden',
                frequencyPrecision:
                    'Stellen Sie die Genauigkeit der angezeigten Frequenzwerte ein.',
                startupPage: 'Die Seite, die nach dem Laden der Anwendung angezeigt wird.',
                railToCollapse:
                    'Auch den leeren Bereich der Navigationsleiste nutzen, um das Menü auszuklappen oder einzuklappen.',
                eyeCandy:
                    'Visuelle Animationen wie drehende Lüftersymbole aktivieren.\nDies beansprucht zusätzliche GPU-Ressourcen.',
                interfaceFont:
                    'Die mit CoolerControl gelieferten Schriftarten verwenden oder die auf Ihrem System konfigurierten.',
                fullScreen: 'Schaltet den Vollbildmodus ein oder aus',
                lineThickness: 'Passen Sie die Linienstärke der Diagramme im Dashboard an',
                startInTray:
                    'Beim Start wird das Hauptfenster der Benutzeroberfläche ausgeblendet und nur\ndas Symbol im Systemtray ist sichtbar.',
                closeToTray:
                    'Wenn Sie das Anwendungsfenster schließen, bleibt die App im Systemtray aktiv',
                zoom: 'Legen Sie manuell den Zoom-Level der Benutzeroberfläche fest.',
                desktopStartupDelay:
                    'Fügt eine Verzögerung vor dem Start der Desktop-Anwendung hinzu (in Sekunden).\nHilft bei Problemen, die dadurch entstehen, dass die Desktop-Anwendung\nautomatisch beim Login gestartet wird oder zu schnell startet',
                unlockRange: 'Werte außerhalb des empfohlenen Bereichs zulassen',
                lockRange: 'Auf den empfohlenen Bereich beschränken',
                applySettingsOnStartup:
                    'Einstellungen automatisch beim Daemon-Start und beim Aufwachen aus dem Ruhezustand anwenden',
                deviceDelayAtStartup:
                    'Verzögerung vor dem Start der Gerätekommunikation (in Sekunden).\nHilft bei Geräten, die Zeit zur Initialisierung benötigen oder nicht durchgängig erkannt werden',
                pollingRate:
                    'Die Rate, mit der Sensordaten abgefragt werden (in Sekunden).\nEine höhere Abfragerate reduziert die Ressourcennutzung, und eine niedrigere erhöht die Reaktionsfähigkeit.\nEine Rate von weniger als 1,0 sollte mit Vorsicht verwendet werden.',
                compressApiPayload: 'API-Payload-Komprimierung aktivieren',
                liquidctlIntegration:
                    'Durch Deaktivieren wird die Liquidctl-Integration vollständig deaktiviert, \nunabhängig vom Installationsstatus des coolercontrol-liqctld \nPakets. Falls verfügbar, werden stattdessen HWMon-Treiber verwendet.',
                liquidctlDeviceInit:
                    'Vorsicht: Deaktivieren Sie dies NUR, wenn Sie oder ein anderes Programm\ndie liquidctl-Geräteinitialisierung handhaben.\nDies kann helfen, Konflikte mit anderen Programmen zu vermeiden.',
                hideDuplicateDevices:
                    'Einige Geräte werden sowohl von Liquidctl- als auch von HWMon-Treibern unterstützt.\nLiquidctl wird standardmäßig wegen seiner zusätzlichen Funktionen verwendet. Um stattdessen HWMon-Treiber zu verwenden,\ndeaktivieren Sie dies und das Liquidctl-Gerät, um Treiberkonflikte zu vermeiden.',
                drivePowerState:
                    'SSDs und HDDs können insbesondere heruntergefahren werden und in einen Energiesparmodus eintreten.\nDiese Option, wenn sie aktiviert ist und das Laufwerk dies unterstützt, wird die Laufwerktemperaturen\nals 0 °C melden, wenn es heruntergefahren ist, damit die Lüfterprofile entsprechend angepasst werden können.',
                sensorsAutoDetect:
                    'Super-I/O-Hardwaresensoren automatisch erkennen und\nKernelmodule beim Start laden. (nur x86_64)',
                sensorsConfig:
                    'Sensornamen und ausgeblendete Sensoren aus den lm-sensors-\nKonfigurationsdateien (/etc/sensors3.conf und /etc/sensors.d) verwenden.\nIn CoolerControl gesetzte Namen haben immer Vorrang.',
                deviceListener:
                    'Auf Geräte-Hinzufügen/-Entfernen-Ereignisse lauschen (z. B. USB-Hotplug)\nund benachrichtigen, wenn Hardwareänderungen erkannt werden.',
                triggersDaemonRestart: 'Löst einen automatischen Daemon-Neustart aus',
                copyThemeCode:
                    'Einen kompakten Code für dein aktuelles benutzerdefiniertes Theme kopieren.\nTeile ihn in Chats oder Foren.',
                pasteThemeCode:
                    'Ein benutzerdefiniertes Theme aus einem geteilten Code (cct1:...) anwenden.',
            },
            applySettingAndRestart:
                'Das Ändern dieser Einstellung erfordert einen Neustart des Daemons und der UI. Möchten Sie das jetzt tun?',
            restartHeader: 'Einstellung anwenden und Neustart',
            success: 'Erfolg',
            successDetail: 'Operation erfolgreich abgeschlossen',
            languageChangeConfirm: 'Sprache ändern?',
            languageChangeConfirmMessage:
                'Sind Sie sicher, dass Sie fortfahren möchten? Wenn einige Oberflächenelemente nicht korrekt angezeigt werden, aktualisieren Sie bitte die Seite manuell.',
            languageChangeSuccess: 'Sprache erfolgreich gewechselt.',
            languageChangeError: 'Fehler beim Ändern der Sprache. Bitte versuchen Sie es erneut.',
            themeChangeSuccess: 'Theme erfolgreich gewechselt.',
        },
        menu: {
            dashboards: 'Dashboards',
            customSensors: 'Benutzerdefinierte Sensoren',
            alerts: 'Warnungen',
            pinned: 'Angeheftet',
            tooltips: {
                createMode: 'Modus aus aktuellen Einstellungen erstellen',
                addProfile: 'Profil hinzufügen',
                addAlert: 'Warnung hinzufügen',
                addDashboard: 'Dashboard hinzufügen',
                duplicate: 'Duplizieren',
                rename: 'Umbenennen',
                addCustomSensor: 'Benutzerdefinierten Sensor hinzufügen',
                addFunction: 'Funktion hinzufügen',
                chooseColor: 'Farbe wählen',
            },
        },
        plugins: {
            plugins: 'Plugins',
            notFound: 'Plugin nicht gefunden',
            type: 'Typ',
            address: 'Adresse',
            privileges: 'Berechtigungen',
            url: 'URL',
            start: 'Starten',
            stop: 'Stoppen',
            restart: 'Neustarten',
            started: 'Plugin gestartet',
            stopped: 'Plugin gestoppt',
            restarted: 'Plugin neugestartet',
            startFailed: 'Plugin konnte nicht gestartet werden',
            stopFailed: 'Plugin konnte nicht gestoppt werden',
            restartFailed: 'Plugin konnte nicht neugestartet werden',
            overview: 'Plugins-Übersicht',
            gettingStarted:
                'Plugins erweitern CoolerControl um zusätzliche Geräteunterstützung, Integrationen und Automatisierung. Sie können neue Gerätesensoren und -steuerungen bereitstellen, sich mit externen Diensten verbinden oder benutzerdefinierte UI-Seiten hinzufügen.',
            findPlugins: 'Plugins finden und installieren',
            restartNote:
                'Wenn Sie kürzlich ein neues Plugin hinzugefügt haben und es hier nicht angezeigt wird, starten Sie den CoolerControl-Daemon neu.',
            containerNote:
                'Wenn CoolerControl in einem Container ausgeführt wird, müssen Plugins im persistierten virtuellen freigegebenen Ordner abgelegt werden, damit sie Container-Neustarts überstehen.',
            installedPlugins: 'Installierte Plugins',
            noPlugins: 'Keine Plugins installiert',
            info: 'Info',
            description: 'Beschreibung',
            enable: 'Aktivieren',
            disable: 'Deaktivieren',
            pluginDisabled: 'Plugin deaktiviert.',
            pluginEnabled: 'Plugin aktiviert.',
            pluginDisabledRestart: 'Plugin deaktiviert. Daemon neu starten, um anzuwenden.',
            pluginEnabledRestart: 'Plugin aktiviert. Daemon neu starten, um anzuwenden.',
            disableFailed: 'Plugin konnte nicht deaktiviert werden',
            enableFailed: 'Plugin konnte nicht aktiviert werden',
            serviceLogs: 'Dienst-Protokolle',
            commandCopied: 'Befehl in die Zwischenablage kopiert',
        },
        add: {
            profile: 'Profil',
            function: 'Funktion',
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
            reconnecting: 'Verbindung wird wiederhergestellt...',
            disconnectedFor: 'Getrennt seit {time}',
        },
        speed: {
            applySetting: 'Einstellung anwenden',
        },
        customSensors: {
            missingSourcesNotice:
                'Die folgenden Temperaturquellen sind nicht mehr vorhanden und werden beim Speichern entfernt: {sources}',
            sensorType: 'Sensortyp',
            mixFunction: 'Mix-Funktion',
            howCalculateValue: 'Wie der resultierende Sensorwert berechnet werden soll',
            tempFile: 'Temperaturdatei',
            filePathTooltip:
                'Geben Sie den absoluten Pfad zur Temperaturdatei ein, die für diesen Sensor verwendet werden soll.\nDie Datei muss das sysfs-Datenformat-Standard verwenden:\nEine Festkommazahl in Milligrad Celsius.\nz.B. 80000 für 80°C.\nDie Datei wird bei der Übermittlung überprüft.',
            browse: 'Durchsuchen',
            browseCustomSensorFile: 'Nach einer benutzerdefinierten Sensordatei suchen',
            tempSources: 'Temp-Quellen',
            tempSource: 'Temp-Quelle',
            tempSourcesTooltip:
                'Temperaturquellen, die in der Mischfunktion verwendet werden sollen<br/><i>Hinweis: Beim Kombinieren mehrerer benutzerdefinierter Sensoren sind nur direkte Eltern-Kind-Beziehungen erlaubt.<br/>Verwenden Sie Mix-Profile für komplexere Setups.</i>',
            offset: 'Versatz',
            offsetTooltip:
                'Geben Sie einen negativen oder positiven Versatz ein, der auf den Quellsensor angewendet wird.<br/><i>Hinweis: Der Endwert wird auf normale Temperaturbereiche begrenzt.</i>',
            timeWindow: 'Glättungsfenster',
            timeWindowTooltip:
                'Wie viele Sekunden der jüngsten Stichproben zusammen geglättet werden sollen.<br/><i>Hinweis: Muss zwischen 1 und 300 Sekunden liegen.</i>',
            helpText: {
                mix: 'Kombiniert mehrere Temperaturquellen mit der gewählten Funktion (Min/Max/Mittel/Differenz/Gewichtetes Mittel). Zur Steuerung von Lüftern nach dem heißesten mehrerer Sensoren oder zum Ausgleich zwischen Zonen.',
                file: 'Liest die Temperatur aus einem Dateipfad. Für Sensoren, die nicht automatisch von CoolerControl erkannt werden.',
                offset: 'Addiert oder subtrahiert einen festen Wert von einer Temperaturquelle. Zur Kalibrierung einer bekannten Sensorungenauigkeit.',
                timeAverage:
                    'Arithmetisches Mittel über ein festes Zeitfenster. Die Ausgabe ist durch den Eingangsbereich begrenzt und überschießt nie. Für Lüfter, die kurze Temperaturspitzen ignorieren sollen.',
                exponentialMovingAvg:
                    'Gewichtetes Mittel mit Bevorzugung neuerer Messwerte. Glatter als der zeitliche Durchschnitt bei gleicher Fenstergröße, benötigt jedoch etwa das 3-fache der Fensterlänge, um einer dauerhaften Änderung vollständig zu folgen. Für Lüfter, die echte Trends ohne Jitter verfolgen sollen.',
            },
            tempWeights: 'Temp-Gewichtungen',
            tempName: 'Temp-Name',
            weight: 'Gewichtung',
            saveCustomSensor: 'Benutzerdefinierten Sensor speichern',
            unsavedChanges:
                'Es gibt ungespeicherte Änderungen an diesem benutzerdefinierten Sensor.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
            selectCustomSensorFile: 'Benutzerdefinierte Sensordatei auswählen',
            deleteCustomSensor: 'Benutzerdefinierten Sensor löschen',
            deleteCustomSensorConfirm:
                'Sind Sie sicher, dass Sie den benutzerdefinierten Sensor löschen möchten: "{name}"?',
        },
        dashboard: {
            timeRange: 'Zeitbereich',
            chartType: 'Diagrammtyp',
            filterSensors: 'Sensoren filtern',
            mouseActions:
                'Dashboard-Mausaktionen:\n- Markieren zum Zoomen.\n- Strg+Scrollen zum Zoomen.\n- Rechtsklick zum Schwenken im gezoomten Zustand.\n- Doppelklick zum Zurücksetzen und fortsetzen der Aktualisierung.\n- Strg+Klick oder Mausklick in der Mitte, um alle Sensoren im Tooltip anzuzeigen.',
            fullPage: 'Vollseite',
            filterTags: 'Tags filtern',
            filterByTag: 'Nach Tag filtern',
            filterBySensor: 'Nach Sensor filtern',
            filterTypes: 'Typen filtern',
            filterByDataType: 'Nach Datentyp filtern',
            exitFullPage: 'Vollseite verlassen',
            deleteDashboard: 'Dashboard löschen',
            deleteDashboardConfirm:
                'Sind Sie sicher, dass Sie das Dashboard löschen möchten: "{name}"?',
            setAsHome: 'Als Startseite festlegen',
            duplicateDashboard: 'Dashboard duplizieren',
            openCooling: 'Kühlungssteuerung öffnen',
        },
        appInfo: {
            noWarranty: 'Dieses Programm kommt absolut ohne Garantie.',
            changeStartupPage: 'Startseite in den Einstellungen ändern',
            daemonStatus: 'Daemon-Status',
            acknowledgeIssues: 'Probleme bestätigen',
            status: 'Status',
            host: 'Host',
            uptime: 'Laufzeit',
            version: 'Version',
            processId: 'Prozess-ID',
            memoryUsage: 'Speichernutzung',
            liquidctl: 'Liquidctl',
            connected: 'Verbunden',
            disconnected: 'Nicht verbunden',
            helpfulLinks: 'Hilfreiche Links',
            uiTour: 'UI-Tour',
            gettingStarted: 'Erste Schritte',
            helpSettingUp: 'Hilfe bei der Einrichtung der Lüftersteuerung',
            gettingStartedStep1:
                'Öffnen Sie Kühlung und wählen Sie den Lüfter, den Sie steuern möchten.',
            gettingStartedStep2:
                'Wählen Sie Geführte Einrichtung und dann Neues Profil, um die Lüfterkurve festzulegen.',
            gettingStartedStep3: 'Verwenden Sie dieses Profil für beliebig viele Lüfter.',
            gettingStartedAutoCreate:
                'Mit {wizard} lassen sich grundlegende Profile für alle Ihre Lüfter auf einmal einrichten.',
            gettingStartedAutoCreateLink: 'Profile automatisch erstellen',
            calibrateFansLink: 'kalibrieren Sie Ihre Lüfter',
            hardwareSupport: 'Hardware-Unterstützung',
            whatsNew: 'Was ist neu',
            logsAndDiagnostics: 'Logs und Diagnose',
            downloadCurrentLog: 'Aktuelle Logs herunterladen',
            deviceHealth: 'Gerätezustand',
            deviceHealthOk: 'Alle Sensoren und Kanäle sind fehlerfrei.',
            detection: 'Chip-Erkennung',
            detectionDescription:
                'Was die Super-I/O-Chipsuche beim Start des Daemons gefunden hat. Module werden beim Start geladen, daher erklärt dieser Durchlauf einen nicht angebundenen Chip.',
            detectionButton: 'Chip-Erkennung',
            detectionNotRun:
                'Es wurde keine Erkennung ausgeführt, daher ist über Super-I/O-Chips auf diesem Rechner nichts bekannt.',
            detectionSecureBoot: 'Secure Boot',
            detectionContainer: 'Container',
            detectionDevPort: '/dev/port verfügbar',
            detectionChips: 'Erkannte Chips',
            detectionNoChips: 'Es wurden keine Super-I/O-Chips erkannt.',
            detectionBlacklisted: 'Blockierte Treiber',
            hardwareSupportOk: 'Alle erkannte Hardware wird unterstützt und ist steuerbar.',
            hardwareReport: 'Hardware-Bericht',
            hardwareReportDescription:
                'Eine Übersicht dessen, was CoolerControl auf diesem Rechner sieht, bereit zum Einfügen in einen Support-Kanal. Seriennummern und Kennungen sind ausgenommen.',
            hardwareReportFull: 'Vollständigen hwmon-Baum einbeziehen',
            hardwareReportEmpty: 'Der Bericht konnte nicht erstellt werden.',
            hardwareReportButton: 'Hardware-Bericht',
            hardwareReportCopy: 'Kopieren',
            hardwareReportCopied: 'Kopiert',
            findingNoDriverBound:
                'Ein Chip wurde erkannt, aber kein geladener Treiber bedient ihn.',
            findingBlacklisted: 'Dieser Treiber steht auf der Blockliste und wurde nicht geladen.',
            findingBlockedByEnvironment:
                'Die Hardware-Erkennung konnte in dieser Umgebung nicht ausgeführt werden.',
            findingBlockedBySecureBoot:
                'Die Hardware-Erkennung konnte nicht ausgeführt werden, da Secure Boot aktiviert ist.',
            findingBlockedByContainer:
                'Die Hardware-Erkennung konnte in einem Container nicht ausgeführt werden.',
            findingBlockedByNoDevPort:
                'Die Hardware-Erkennung konnte nicht ausgeführt werden, da /dev/port nicht verfügbar ist.',
            findingDetectionUnsupported:
                'Die Hardware-Erkennung wird auf dieser Architektur nicht unterstützt.',
            failsafeActive: 'Failsafe-Werte in Verwendung',
            deviceUnreachable: 'Gerät antwortet nicht',
            deviceUnreachableDetail:
                'Der Treiber antwortet nicht mehr, daher kann dieses Gerät weder ausgelesen noch gesteuert werden. Es wird regelmäßig erneut versucht.',
            missingTempSource: 'Fehlende Temperaturquelle',
            staleTempSource: 'Temperaturquelle verwendet Failsafe-Werte',
            stressTest: 'Thermische Stresstests',
            stressTestTooltip:
                'Erzeugt anhaltende Thermallast zur Validierung\nvon Lüfterkurven und Kühlprofilen.\nErgebnisse können je nach Hardware variieren.\nInstallieren Sie stress-ng für zusätzliche Backends.',
            cpuStress: 'CPU-Stress',
            gpuStress: 'GPU-Stress',
            gpuStressTooltip:
                'Erfordert Vulkan- oder OpenGL ES-Treiber<br>bei Verwendung des eingebauten Backends.',
            ramStress: 'RAM-Stress',
            driveStress: 'Laufwerk-Stress',
            driveStressTooltip:
                'I/O-Stress auf einem Blockgerät zur<br>Wärmeerzeugung an Laufwerkscontrollern.<br>stress-ng erfordert, dass das Gerät eingehängt ist.',
            builtInBackend: 'eingebaut',
            stressNgBackend: 'stress-ng',
            backendTooltip:
                'Wählen Sie das Stresstest-Backend.<br>Das eingebaute Backend funktioniert ohne externe Abhängigkeiten.<br>stress-ng (sofern installiert) bietet zusätzliche Stressor-Varianten.',
            selectDrive: 'Laufwerk auswählen',
            selectGpu: 'Grafikkarte auswählen',
            allGpus: 'Alle Grafikkarten',
            start: 'Start',
            stop: 'Stop',
            stopAll: 'Alle stoppen',
            active: 'Aktiv',
            inactive: 'Inaktiv',
            psuWarningHeader: 'Warnung: Hohe Leistungsaufnahme',
            psuWarningMessage:
                'Das gleichzeitige Ausführen von CPU- und GPU-Stresstests belastet das Netzteil erheblich. Bei Übertaktung oder einem Netzteil mit niedriger Wattzahl kann es zu Systeminstabilität kommen. Möchten Sie trotzdem fortfahren?',
            proceed: 'Fortfahren',
        },
        alerts: {
            triggersOutside: 'löst unter {min} oder über {max}{unit} aus',
            triggersAbove: 'löst über {max}{unit} aus',
            stateSince: '{state} seit {time}',
            deleteAlert: 'Warnung löschen',
            duplicateAlert: 'Warnung duplizieren',
            alertsOverview: 'Warnungsübersicht',
            alertLogs: 'Warnungsprotokolle',
            alertTriggered: 'Warnung ausgelöst',
            alertRecovered: 'Warnung wiederhergestellt',
            alertError: 'Warnungsfehler',
            alertSensorsReadable: 'Warnungssensoren wieder lesbar',
            deleteAlertConfirm: 'Sind Sie sicher, dass Sie löschen möchten: "{name}"?',
            saveAlert: 'Warnung speichern',
            channelSources: 'Kanalquellen für Warnung',
            channelSourcesTooltip:
                'Die von dieser Warnung überwachten Kanalquellen.\nEin Sensortyp pro Warnung: die erste Auswahl filtert die übrigen.',
            triggerConditions: 'Auslösebedingungen',
            maxValueTooltip: 'Werte über diesem lösen die Warnung aus.',
            minValueTooltip: 'Werte unter diesem lösen die Warnung aus.',
            warmupDurationTooltip:
                'Gibt an, wie lange eine Bedingung aktiv sein muss, bevor die Warnung als aktiv gilt.\nDie Überprüfung erfolgt nur in regelmäßigen Poll-Intervallen\nund kann daher von dieser Länge abweichen.',
            cooldownDurationTooltip:
                'Gibt an, wie lange der Wert wieder im Bereich bleiben muss, bevor die Warnung wiederhergestellt wird.\nVerhindert schnelles Hin- und Herspringen zwischen ausgelöst und wiederhergestellt.',
            cooldownLessThan: 'Bedingung wiederhergestellt länger als',
            repeatInterval: 'Benachrichtigung wiederholen alle',
            repeatIntervalTooltip:
                'Die Desktop-Benachrichtigung in diesem Intervall erneut senden, solange die Warnung aktiv bleibt.\n0 deaktiviert wiederholte Benachrichtigungen.',
            enabled: 'Aktiviert',
            enabledTooltip: 'Eine deaktivierte Warnung wird überhaupt nicht ausgewertet.',
            sectionGeneral: 'Allgemein',
            sectionNotifications: 'Benachrichtigungen',
            sectionActions: 'Aktionen',
            silence: 'Stummschalten',
            silenceTooltip:
                'Stummschalten: Benachrichtigungen und Herunterfahren für eine Weile unterdrücken.\nDie Warnung wird weiterhin ausgewertet und zeigt ihren Zustand an.',
            silence15m: 'Für 15 Minuten stummschalten',
            silence1h: 'Für 1 Stunde stummschalten',
            silence8h: 'Für 8 Stunden stummschalten',
            silence24h: 'Für 24 Stunden stummschalten',
            unsilence: 'Stummschaltung jetzt aufheben',
            enableAlert: 'Warnung aktivieren',
            disableAlert: 'Warnung deaktivieren',
            silencedUntil: 'Stummgeschaltet bis {time}',
            disabledLabel: 'Deaktiviert',
            greaterThan: 'größer als',
            lessThan: 'kleiner als',
            newAlert: 'Neue Warnung',
            warmupGreaterThan: 'bedingung ausgelöst länger als',
            unsavedChanges: 'Es gibt ungespeicherte Änderungen an dieser Warnung.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
            desktopNotify: 'Desktop-Benachrichtigung',
            desktopNotifyTooltip:
                'Desktop-Benachrichtigungen aktivieren, wenn die Warnung ausgelöst wird.\n(Falls unterstützt)',
            desktopNotifyRecovery: 'Desktop-Benachrichtigung bei Wiederherstellung',
            desktopNotifyRecoveryTooltip:
                'Desktop-Benachrichtigungen aktivieren, wenn die Warnung wiederhergestellt wird.\n(Falls unterstützt)',
            desktopNotifyAudio: 'Desktop-Benachrichtigungston',
            desktopNotifyAudioTooltip:
                'Desktop-Benachrichtigungston aktivieren, wenn die Warnung ausgelöst wird.\n(Falls unterstützt)',
            shutdownOnActivation: 'Herunterfahren bei Aktivierung',
            shutdownOnActivationTooltip:
                'System-Herunterfahren aktivieren, wenn die Warnung ausgelöst wird.\nDas System wird eine Minute nach Auslösung der Warnung heruntergefahren\nund abgebrochen, wenn die Warnung wiederhergestellt wird.',
        },
        profiles: {
            targetDuty: 'Ziel',
            actualDuty: 'Ist',
            targetHint:
                'Das Ziel wird aus den aktuellen Temperaturen berechnet, bevor die Funktion des Kanals angewendet wird. Glättung und Hysterese können den tatsächlichen Wert abweichen lassen.',
            createProfile: 'Profil erstellen',
            deleteProfile: 'Profil löschen',
            profileType: 'Profiltyp',
            fixedDuty: 'Feste Lüftergeschwindigkeit',
            tempSource: 'Temperaturquelle',
            memberProfiles: 'Mitgliedsprofile',
            mixFunction: 'Mischfunktion',
            applyMixFunction: 'Mischfunktion auf ausgewählte Profile anwenden',
            profilesToMix: 'Profile zum Mischen',
            saveProfile: 'Profil speichern',
            function: 'Funktion',
            functionToApply: 'Anzuwendende Funktion',
            graphProfileMouseActions:
                'Grafikprofil Mausaktionen:\n- Strg+Scrollen zum Zoomen.\n- Linksklick auf Linie um Punkt hinzuzufügen.\n- Rechtsklick auf Punkt zum Entfernen.\n- Punkt ziehen zum Verschieben.',
            unsavedChanges: 'Es gibt ungespeicherte Änderungen an diesem Profil.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
            newProfile: 'Neues Profil',
            tooltip: {
                profileType:
                    'Profiltypen:<br/>- Standard: Ungesteuert, gibt die Kontrolle an den Gerätetreiber zurück<br/>- Fest: Setzt eine konstante Geschwindigkeit<br/>- Graph: Anpassbare Lüfterkurve<br/>- Mix: Kombiniert mehrere Profile<br/>- Overlay: Überlagert die Ausgabe eines bestehenden Profils mit einem Versatz',
            },
            profileDeleted: 'Profil gelöscht',
            profileDuplicated: 'Profil dupliziert',
            usedBy: 'Verwendet von',
            deleteProfileConfirm: 'Sind Sie sicher, dass Sie löschen möchten: "{name}"?',
            deleteProfileWithChannelsConfirm:
                '"{name}" wird derzeit verwendet von: {channels}.\nDas Löschen dieses Profils wird die Einstellungen dieser Kanäle zurücksetzen.\nSind Sie sicher, dass Sie "{name}" löschen möchten?',
            profileUpdated: 'Profil erfolgreich aktualisiert',
            profileUpdateError:
                'Bei dem Versuch, dieses Profil zu aktualisieren, ist ein Fehler aufgetreten',
            tempSourceRequired: 'Für ein Grafikprofil ist eine Temperaturquelle erforderlich.',
            memberProfilesRequired:
                'Für ein Mischprofil sind mindestens 2 Mitgliedsprofile erforderlich.',
            minProfileTemp: 'Minimale Profiltemperatur',
            maxProfileTemp: 'Maximale Profiltemperatur',
            staticOffset: 'Statischer Versatz',
            offsetType: 'Versatztyp',
            offsetTypeStatic: 'Statischer Versatz',
            offsetTypeGraph: 'Graph-Versatz',
            baseProfile: 'Basisprofil',
            baseProfileRequired: 'Für ein Overlay-Profil ist ein Basisprofil erforderlich.',
            profileOutputDuty: 'Profil-Ausgabeleistung',
            offsetDuty: 'Versatzleistung',
            points: 'Punkte',
            moveTable: 'In andere Ecke verschieben',
            addPointAfter: 'Punkt danach hinzufügen',
            removePoint: 'Punkt entfernen',
            curvePointLimitBadge: 'max. {n} Pkt.',
            curveLimitedByAmdGpu:
                'Kurve auf {n} Punkte durch AMD GPU Hardware-Lüfterkurve begrenzt.',
            curveLimitedByFirmware:
                'Kurve auf {n} Punkte durch Geräte-Firmware-Lüfterkurve begrenzt.',
        },
        modes: {
            createMode: 'Modus erstellen',
            editMode: 'Modus bearbeiten',
            updateToCurrent: 'Aktuelle Einstellungen im Modus speichern',
            deleteMode: 'Modus löschen',
            deleteModeConfirm: 'Sind Sie sicher, dass Sie den Modus löschen möchten: "{name}"?',
            updateModeConfirm:
                'Sind Sie sicher, dass Sie "{name}" mit der aktuellen Konfiguration überschreiben möchten?',
            duplicateMode: 'Modus duplizieren',
        },
        functions: {
            createFunction: 'Funktion erstellen',
            deleteFunction: 'Funktion löschen',
            saveFunction: 'Funktion speichern',
            stepSizeTitle: 'Schrittgröße',
            fixedStepSize: 'Fest',
            fixedStepSizeTooltip:
                'Aktiviert verwendet eine feste Schrittgröße für alle Änderungen.\nDeaktiviert ermöglicht die Einstellung eines Mindest- und Höchstbereichs für die Schrittgröße.',
            asymmetric: 'Asymmetrisch',
            asymmetricTooltip:
                'Wenn aktiviert, können separate Schrittgrößenlimits für Geschwindigkeitserhöhungen und -verringerungen konfiguriert werden.\nNützlich, wenn Lüfter schnell hochfahren, aber langsam herunterfahren sollen, oder umgekehrt.',
            stepSizeMin: 'Minimum',
            stepSizeMinTooltip:
                'Die kleinste Lüftergeschwindigkeitsänderung, die angewendet wird.\nKleinere Änderungen werden ignoriert, um unnötige Anpassungen zu reduzieren.',
            stepSizeMax: 'Maximum',
            stepSizeMaxTooltip:
                'Die größte erlaubte Lüftergeschwindigkeitsänderung pro Aktualisierung.\nGrößere Änderungen werden auf diesen Wert begrenzt für sanftere Übergänge.',
            stepSizeFixed: 'Größe',
            stepSizeFixedTooltip:
                'Eine einzelne Schrittgröße für alle Lüftergeschwindigkeitsänderungen.\nAlle Anpassungen werden auf genau diesen Wert begrenzt.',
            stepSizeFixedIncreasing: 'Steigend',
            stepSizeFixedIncreasingTooltip:
                'Feste Schrittgröße bei steigender Lüftergeschwindigkeit.\nAlle Aufwärtsanpassungen werden auf genau diesen Wert begrenzt.',
            stepSizeFixedDecreasing: 'Fallend',
            stepSizeFixedDecreasingTooltip:
                'Feste Schrittgröße bei fallender Lüftergeschwindigkeit.\nAlle Abwärtsanpassungen werden auf genau diesen Wert begrenzt.',
            stepSizeMinIncreasing: 'Minimum Steigend',
            stepSizeMinIncreasingTooltip:
                'Minimale Schrittgröße bei steigender Lüftergeschwindigkeit.\nKleinere berechnete Änderungen werden ignoriert, um unnötige Anpassungen zu reduzieren.',
            stepSizeMaxIncreasing: 'Maximum Steigend',
            stepSizeMaxIncreasingTooltip:
                'Maximale Schrittgröße bei steigender Lüftergeschwindigkeit.\nBegrenzt, wie schnell Lüfter pro Aktualisierung hochfahren können.',
            stepSizeMinDecreasing: 'Minimum Fallend',
            stepSizeMinDecreasingTooltip:
                'Minimale Schrittgröße bei fallender Lüftergeschwindigkeit.\nKleinere berechnete Änderungen werden ignoriert, um unnötige Anpassungen zu reduzieren.',
            stepSizeMaxDecreasing: 'Maximum Fallend',
            stepSizeMaxDecreasingTooltip:
                'Maximale Schrittgröße bei fallender Lüftergeschwindigkeit.\nBegrenzt, wie schnell Lüfter pro Aktualisierung herunterfahren können.',
            hysteresis: 'Erweiterte Hysterese',
            hysteresisThreshold: 'Schwellenwert',
            hysteresisThresholdTooltip:
                'Minimale Temperaturänderung (°C), die erforderlich ist, bevor die Lüftergeschwindigkeit angepasst wird.\nHilft, schnelle Lüftergeschwindigkeitsschwankungen durch kleine Temperaturvariationen zu verhindern.',
            hysteresisDelay: 'Verzögerung',
            hysteresisDelayTooltip:
                'Reaktionsverzögerung (Sekunden) vor der Anwendung von Lüftergeschwindigkeitsänderungen.\nTemporäre Temperaturspitzen innerhalb dieser Verzögerung werden ignoriert, um Schwankungen zu glätten.',
            onlyDownward: 'Nur Abwärts',
            onlyDownwardTooltip: 'Hysterese-Einstellungen nur anwenden, wenn die Temperatur sinkt.',
            stepOverrides: 'Schritt-Überschreibungen',
            thresholdHopping: 'Schwellenwert-Überspringen',
            thresholdHoppingTooltip:
                'Wenn die Lüftergeschwindigkeit 30+ Sekunden unverändert bleibt, werden die minimale Schrittgröße und Hysterese-Limits vorübergehend umgangen.\nDies stellt sicher, dass Lüfter schließlich ihre Zielgeschwindigkeit erreichen, auch bei konservativen Schwellenwerteinstellungen. Die maximale Schrittgröße wird immer eingehalten.',
            bypassMinAtExtremes: 'Immer 0% / 100% anwenden',
            bypassMinAtExtremesTooltip:
                'Wenn aktiviert, werden Ziel-Drehzahlen von 0% oder 100% auch dann angewendet, wenn die Änderung kleiner als die minimale Schrittgröße ist.\nNützlich, um sicherzustellen, dass Lüfter vollständig stoppen oder die maximale Drehzahl erreichen. Standardmäßig deaktiviert.',
            unsavedChanges: 'Es gibt ungespeicherte Änderungen an dieser Funktion.',
            unsavedChangesHeader: 'Ungespeicherte Änderungen',
            functionError: 'Fehler beim Versuch, diese Funktion zu aktualisieren',
            newFunction: 'Neue Funktion',
            functionDeleted: 'Funktion gelöscht',
            functionDuplicated: 'Funktion dupliziert',
            usedBy: 'Verwendet von',
            deleteFunctionConfirm: 'Sind Sie sicher, dass Sie "{name}" löschen möchten?',
            deleteFunctionWithProfilesConfirm:
                '"{name}" wird derzeit von den Profilen verwendet: {profiles}.\nDas Löschen dieser Funktion wird die Funktionen dieser Profile zurücksetzen.\nSind Sie sicher, dass Sie "{name}" löschen möchten?',
        },
        error: {
            accessDenied: 'Zugriff verweigert',
            accessDeniedMessage:
                'Authentifizierung fehlgeschlagen. Bitte überprüfen Sie Ihr Passwort und versuchen Sie es erneut.',
            connectionError: 'CoolerControl-Verbindungsfehler',
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
            gifNotSupported:
                'Die Firmware dieses Bildschirms kann keine GIFs anzeigen. Bitte ein statisches Bild wählen.',
        },
        shortcuts: {
            browserHint:
                'Verwenden Sie in einem Webbrowser stattdessen Strg+Alt+Zahl (Browser reservieren Strg+Zahl für den Tab-Wechsel).',
            shortcuts: 'Tastenkombinationen',
            ctrl: 'Strg',
            comma: ',',
            viewShortcuts: 'Tastenkombinationen',
            settings: 'Einstellungen',
        },
    },
    components: {
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
            forgotPassword: 'Passwort vergessen?',
            forgotPasswordHelpIntro:
                'Führen Sie diesen Befehl als Root im Terminal aus und klicken Sie dann auf "UI neu laden":',
            forgotPasswordCopyCommand: 'Befehl kopieren',
            forgotPasswordCommandCopied: 'Befehl in die Zwischenablage kopiert',
            forgotPasswordReloadButton: 'UI neu laden',
            continueButton: 'Weiter',
            backButton: 'Zurück',
            passwordMismatch: 'Passwörter stimmen nicht überein',
        },
        notFound: {
            message: 'Genau wie die perfekte Linux 🐧 Distribution\nexistiert diese Seite nicht.',
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
            search: 'Suche',
            searchDesc:
                'Finden Sie hier jedes Gerät, jeden Sensor, jede Einstellung und jede Aktion. Mit Strg+K von überall in der App erreichbar.',
            welcome: 'Willkommen bei CoolerControl!',
            gettingStartedIntro:
                'Machen Sie eine kurze Tour, um sich zu orientieren. Sie führt durch die Navigationsleiste und die wichtigsten Bereiche der Anwendung.',
            startTourAgain:
                'Sie können diese Tour jederzeit über die Einstellungen erneut starten.',
            startTour: 'Tour starten',
            maybeLater: 'Vielleicht später',
            openGettingStarted: 'Erste-Schritte-Dokumentation öffnen',
            finishLater: 'Alles klar, danke',
            home: 'Start',
            homeDesc:
                'Die Startseite der Anwendung: Daemon-Status und Gerätezustand auf einen Blick, dazu Protokolle, App-Infos, hilfreiche Links und Stresstest-Werkzeuge.',
            cooling: 'Kühlung',
            coolingDesc:
                'Ihre Zentrale für die Lüftersteuerung: Passen Sie Lüftergeschwindigkeiten und Pumpen an und wenden Sie Profile und Funktionen auf jeden Kanal an.',
            monitoring: 'Überwachung',
            monitoringDesc:
                'Erstellen Sie Dashboards, beobachten Sie jeden Sensor und richten Sie Warnungen ein, um Ihr System in Echtzeit zu verfolgen.',
            devices: 'Geräte',
            devicesDesc:
                'Überprüfen Sie erkannte Hardware, konfigurieren Sie gerätespezifische Funktionen wie RGB-Beleuchtung und LCD-Bildschirme und erstellen Sie benutzerdefinierte Sensoren.',
            plugins: 'Plugins',
            pluginsDesc:
                'Durchsuchen und öffnen Sie installierte Plugins, die CoolerControl erweitern.',
            settings: 'Einstellungen',
            settingsDesc:
                'Konfigurieren Sie UI-Einstellungen, Daemon-Optionen und Systemverhalten.',
            access: 'Zugriff',
            accessDesc:
                'Melden Sie sich an oder ab und ändern Sie Ihr Passwort. Hier verwalten Sie auch Zugriffstoken, die Werkzeugen und Plugins API-Zugriff gewähren.',
            restartMenu: 'Neustart-Menü',
            restartMenuDesc: 'UI neu laden oder Systemdaemon bei Bedarf neu starten.',
            modes: 'Modi',
            modesDesc:
                'Modi sind gespeicherte Sammlungen Ihrer Einstellungen. Wechseln Sie hier zwischen Konfigurationen wie Leise und Leistung oder verwalten Sie sie.',
            thatsIt: "Das war's!",
            startNow:
                'Sie sind startklar. Öffnen Sie die Erste-Schritte-Dokumentation, um mehr zu erfahren, oder legen Sie los und konfigurieren Sie Ihre Geräte.',
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
            range: 'Bereich',
            average: 'Durchschnitt',
            resetStats: 'Zurücksetzen',
            resetStatsTooltip: 'Min/Max/Durchschnitt für alle Kanäle zurücksetzen',
        },
        modeTable: {
            setting: 'Einstellung',
        },
        menuTagAssign: {
            title: 'Tags zuweisen',
            noTags: 'Noch keine Tags.',
            tagName: 'Tag-Name',
            editTag: 'Tag bearbeiten',
            deleteTag: 'Tag löschen',
        },
        wizards: {
            calibration: {
                title: 'Lüfter kalibrieren',
                pickIntro:
                    'Wählen Sie die zu kalibrierenden Lüfter aus. Bereits kalibrierte und firmware-gesteuerte Lüfter sind standardmäßig nicht ausgewählt.',
                noFans: 'Keine steuerbaren Lüfter erkannt.',
                selectAll: 'Alle auswählen',
                calibratedBadge: 'kalibriert',
                firmwareControlledBadge: 'firmware-gesteuert',
                firmwareControlledDesc:
                    'Die Firmware steuert das Profil dieses Kanals. Eine Kalibrierung wirkt weiterhin: Ihre Tastgrad-Zuordnung wird in die an die Firmware übergebene Kurve eingerechnet. Der Anlaufimpuls dagegen nicht, da eine Firmware-Kurve ihn nicht abbilden kann.',
                blockedByAlert: "blockiert: Warnung '{name}' ist aktiv",
                alertsPausedNote:
                    '{count} Warnung(en) überwachen die ausgewählten Lüfter und werden während des Durchlaufs der einzelnen Lüfter pausiert.',
                idleNote:
                    'Bei der Kalibrierung wird jeder Lüfter über seinen gesamten Bereich hochgefahren. Am besten im Leerlauf ausführen: Es ist laut und dauert einige Minuten pro Lüfter.',
                concurrencyLabel: 'Lüfter gleichzeitig',
                concurrencyNote:
                    'Mehrere gleichzeitig sind schneller, aber benachbarte Lüfter können die Messwerte des jeweils anderen verfälschen (Querströmung, Push-Pull). Einzeln ist am genauesten.',
                start: 'Start',
                close: 'Schließen',
                running: 'Kalibriere {current} von {total}...',
                queued: 'In Warteschlange',
                done: 'Fertig',
                failed: 'Fehlgeschlagen',
                skipped: 'Übersprungen',
                startFailed: 'Konnte nicht gestartet werden',
                summary: '{done} kalibriert, {failed} fehlgeschlagen, {skipped} übersprungen.',
                reloadBatch:
                    '{count} Lüfter kalibriert. Neu laden, um die neue RPM-normalisierte Steuerung anzuwenden?',
                stagePreflight: 'Vorprüfung',
                stageUpSweep: 'Aufwärts-Sweep',
                stageDownSweep: 'Abwärts-Sweep',
                stageFinalizing: 'Abschluss',
            },
            fanControl: {
                fanControlWizard: 'Fan Control Wizard',
                editCurrentProfile: 'Profil bearbeiten',
                editCurrentFunction: 'Funktion bearbeiten',
                currentSettings: 'Aktuelle Einstellungen',
                manualSpeed: 'Manuelle Geschwindigkeit',
                createNewProfile: 'Neues Profil',
                existingProfile: 'Profil wählen',
                resetSettings: 'Auf Ungesteuert zurücksetzen',
                chooseProfileNameType: 'Wählen Sie einen Profilnamen und einen Typ',
                newDefaultProfile: 'Neues Standardprofil',
                profileCreatedApplied: 'Profil erstellt und angewendet',
                willCreatedAndAppliedTo: 'wird erstellt und angewendet auf',
                newFixedProfile: 'Neues festes Profil',
                withSettings: 'mit den folgenden Einstellungen',
                selectSpeed: 'Wählen Sie Ihre Geschwindigkeit',
                newMixProfile: 'Neues Mix-Profil',
                newGraphProfile: 'Neues Graph-Profil',
                newOverlayProfile: 'Neues Overlay-Profil',
                functionFor: 'Wählen Sie eine anzuwendende Funktion aus',
                functionDescription:
                    'Funktionen passen an, wie Ihr Profil angewendet wird, z.B. Reaktionszeit und Mindestgeschwindigkeit.',
                createNewFunction: 'Neue Funktion',
                existingFunction: 'Funktion wählen',
                defaultFunction: 'Standardfunktion',
                chooseFunctionName: 'Wähle einen Funktionsnamen',
                newFunctionName: 'Funktion für {profileName}',
                summary: 'Zusammenfassung',
                aNewProfile: 'Ein neues Profil',
                andFunction: 'und Funktion',
            },
            profile: {
                willCreated: 'wird erstellt.',
            },
            profileApply: {
                applyProfile: 'Profil anwenden',
                channelsApply: 'Kanäle für Profilanwendung',
                selectChannels: 'Kanäle auswählen',
                channelsTooltip:
                    'Wählen Sie einen oder mehrere Kanäle aus, auf die dieses Profil angewendet werden soll.',
                selectByTag: 'Nach Tag auswählen',
                selectByChannel: 'Nach Kanal auswählen',
                tagFanCount: '{count} Kanal | {count} Kanäle',
                noTags: 'Keine Tags konfiguriert.',
            },
            functionApply: {
                applyFunction: 'Funktion anwenden',
                profilesApply: 'Profile für Funktionsanwendung',
                selectProfiles: 'Profile auswählen',
                profilesTooltip:
                    'Wählen Sie ein oder mehrere Profile aus, auf die diese Funktion angewendet werden soll.',
            },
            generate: {
                title: 'Profile automatisch erstellen',
                assignIntro:
                    'Weisen Sie jedem Lüfter eine Rolle zu. Lassen Sie einen Lüfter ohne Auswahl, um ihn zu überspringen.',
                calibrateFirst:
                    'Für beste Gleichmäßigkeit zuerst die Lüfter kalibrieren (einige Minuten)',
                skip: 'Überspringen',
                noFans: 'Keine steuerbaren Lüfter erkannt.',
                tempsIntro:
                    'Wählen Sie die Temperaturen, denen Ihr Setup folgen soll. Lassen Sie eine leer, um sie auszuschließen: Ein System mit integrierter Grafik braucht keine GPU-Temp., und erst deren Auswahl bezieht die GPU in die Kurven von AIO-Radiator und Gehäuselüftern ein.',
                cpuTemp: 'CPU-Temp.',
                gpuTemp: 'GPU-Temp.',
                liquidTemp: 'Flüssigkeitstemp.',
                ambientTemp: 'Umgebungstemp. (optional)',
                tempNone: 'Keine',
                presetIntro: 'Wählen Sie, wie aggressiv die Lüfter hochdrehen sollen.',
                perKindOverrides: 'Überschreibungen pro Rolle (erweitert)',
                cfmCaveat:
                    'Die Überdruck-Vorgabe basiert auf der Leistung (Duty), nicht auf dem Luftstrom: Bei ungleichen Lüfterzahlen kann kein Überdruck garantiert werden.',
                previewIntro:
                    'Überprüfen Sie, was erstellt und angewendet wird. Es wird nichts gespeichert, bis Sie bestätigen.',
                previewAssignments: 'Lüfterzuweisungen',
                reusedHeader: 'Bereits vorhanden',
                reused: 'wiederverwendet',
                willCreateHeader: 'Wird erstellt',
                startingPointNote:
                    'Ein allgemeiner Ausgangspunkt für Ihr Lüfter-Setup, gedacht zum Anpassen und nicht zum unveränderten Übernehmen.',
                replaces: 'ersetzt {name}',
                generated: '{count} Profile erstellt.',
                generateError: 'Profile konnten nicht erstellt werden.',
                applyError: 'Die Profile konnten nicht erstellt werden.',
                kind: {
                    CpuCooler: 'CPU-Luftkühler',
                    GpuFan: 'GPU-Lüfter',
                    AioRadiator: 'AIO-Radiator',
                    AioPump: 'AIO-Pumpe',
                    CaseIntake: 'Gehäuse-Zuluft',
                    CaseExhaust: 'Gehäuse-Abluft',
                    LaptopFan: 'Laptop-Lüfter',
                },
            },
        },
        channelExtensionSettings: {
            title: 'Gerätekanal-Einstellungen',
            firmwareControlledProfile: 'Firmware-gesteuertes Profil',
            firmwareControlledProfileDesc:
                'Wenn aktiviert, steuert die Geräte-Firmware das Lüfterprofil.\nNützlich bei Hardware, die auf häufige softwareseitige Geschwindigkeitsänderungen nicht gut reagiert.\nNur verfügbar für Graph-Profile, die geräteinterne Temperatursensoren verwenden.\nFunktions-Einstellungen gelten nicht.\nBei einem kalibrierten Kanal werden die Kurvenpunkte über die Kalibrierung umgerechnet, der Anlaufimpuls gilt jedoch nicht.',
            saveError: 'Speichern der Einstellungen der Kanalerweiterung fehlgeschlagen',
            firmwareControlDisabled:
                'Die Firmware-Steuerung ist mit den aktuellen Einstellungen nicht verfügbar.\nVerwenden Sie ein Graph-Profil für dieses Gerät mit einem unterstützten internen Temperatursensor.',
            calibration: {
                heading: 'Drehzahl-Kalibrierung',
                description:
                    'Lassen Sie den Lüfter den gesamten Bereich durchlaufen, um seine tatsächliche Tastgrad-zu-Drehzahl-Kurve zu ermitteln, und steuern Sie den Kanal anschließend als drehzahlnormierten Echt-Tastgrad.\nBeseitigt tote Zonen bei niedrigem Tastgrad und Sättigung bei hohem Tastgrad.\nDer Anlauf-Boost wird ebenfalls automatisch behandelt, sobald der Lüfter kalibriert ist: Ein kurzer Startschub bringt den Lüfter aus dem Stillstand auf Drehzahl, bevor er sich auf den Zieltastgrad einpendelt.\nDer Durchlauf dauert in der Regel mehrere Minuten und kann bei träge reagierenden Lüftern deutlich länger laufen. Der Kanal wird zu Beginn auf 0 % gesetzt.',
                statusNotCalibrated: 'Nicht kalibriert',
                blockedByAlert:
                    "Kalibrierung blockiert: Warnung '{name}' ist auf diesem Lüfter aktiv.",
                alertsPausedNote:
                    'Warnungen, die diesen Lüfter überwachen, werden während des Durchlaufs pausiert.',
                statusInProgress: 'Kalibrierung läuft: {stage} ({percent} %)',
                statusCompleted: 'Kalibriert (gleichmäßig, Abbildung aktiv)',
                statusCompletedStepped: 'Kalibriert (Stufenkurve, Abbildung deaktiviert)',
                statusCompletedWithWarnings: 'Kalibriert mit Warnungen: {messages}',
                statusFailed: 'Letzter Versuch fehlgeschlagen: {message}',
                warningNoTachometer:
                    'keine Drehzahl erkannt (Sensor oder Verkabelung möglicherweise getrennt)',
                warningNotControllable:
                    'Lüfter reagiert nicht auf Tastgrad (vermutlich BIOS-gesteuert)',
                warningLimitedRange:
                    'eingeschränkter Drehzahlbereich ({span} RPM); grobe Abbildungs-Auflösung',
                warningOscillating:
                    'Lüfter oszilliert zwischen {lower} % und {upper} % Tastgrad (firmware-gesteuerter Anlaufschub); Abbildung bei niedrigem Tastgrad deaktiviert',
                warningImplausibleCurve:
                    'gemessene Kurve ist flach, invertiert oder ihr Minimum liegt über halbem Tastgrad (Drehzahlwerte wirken unzuverlässig); Abbildung deaktiviert, zum erneuten Versuch neu kalibrieren',
                stagePreflight: 'Vorprüfung',
                stageUpSweep: 'Aufwärts-Durchlauf',
                stageDownSweep: 'Abwärts-Durchlauf',
                stageFinalizing: 'Abschluss',
                buttonCalibrate: 'Kalibrieren',
                buttonRecalibrate: 'Neu kalibrieren',
                buttonCancel: 'Abbrechen',
                buttonClear: 'Zurücksetzen',
                clearConfirm:
                    'Kalibrierung für {channel} löschen? Ein erneuter Durchlauf dauert mehrere Minuten.',
                buttonViewCurve: 'Kurve anzeigen',
                caveatsBanner:
                    'Mehrere primäre Kühllüfter gleichzeitig zu kalibrieren kann die Systemtemperatur erhöhen.\nGleichzeitig diagnostizierte Push-Pull-Radiatorlüfter können ungenaue Messwerte liefern.\nHalten Sie das System während der Kalibrierung im Leerlauf.',
                clearedNotice:
                    'Zurückgesetzt. Lüfterkurven auf diesem Kanal steuern jetzt wieder direkt den Geräte-Tastgrad.',
                startError: 'Kalibrierung konnte nicht gestartet werden',
                cancelError: 'Kalibrierung konnte nicht abgebrochen werden',
                clearError: 'Kalibrierung konnte nicht zurückgesetzt werden',
                reloadHeader: 'Oberfläche neu laden',
                reloadAccept: 'Neu laden',
                reloadReject: 'Später',
                reload_rpm_only_completed_single:
                    'Kalibrierung für {channelName} abgeschlossen. Laden Sie die Oberfläche neu, um den Tastgrad-Verlauf des Kanals anzuzeigen.',
                reload_rpm_only_completed_multi:
                    'Kalibrierung für {channelList} abgeschlossen. Laden Sie die Oberfläche neu, um den Tastgrad-Verlauf der einzelnen Kanäle anzuzeigen.',
                reload_rpm_only_cleared_single:
                    'Kalibrierung für {channelName} zurückgesetzt. Laden Sie die Oberfläche neu, um den nun veralteten Tastgrad-Verlauf des Kanals zu entfernen.',
                reload_rpm_only_cleared_multi:
                    'Kalibrierung für {channelList} zurückgesetzt. Laden Sie die Oberfläche neu, um den nun veralteten Tastgrad-Verlauf der einzelnen Kanäle zu entfernen.',
                reload_duty_range_completed_single:
                    'Kalibrierung für {channelName} abgeschlossen. Laden Sie die Oberfläche neu, damit der Tastgrad-Regler und der Lüftersteuerungs-Assistent den neuen Tastgradbereich des Kanals übernehmen.',
                reload_duty_range_completed_multi:
                    'Kalibrierung für {channelList} abgeschlossen. Laden Sie die Oberfläche neu, damit der Tastgrad-Regler und der Lüftersteuerungs-Assistent den neuen Tastgradbereich jedes Kanals übernehmen.',
                reload_duty_range_cleared_single:
                    'Kalibrierung für {channelName} zurückgesetzt. Laden Sie die Oberfläche neu, damit der Tastgrad-Regler wieder auf die Hardware-Grenzen des Kanals zurückspringt.',
                reload_duty_range_cleared_multi:
                    'Kalibrierung für {channelList} zurückgesetzt. Laden Sie die Oberfläche neu, damit der Tastgrad-Regler wieder auf die Hardware-Grenzen jedes Kanals zurückspringt.',
                reload_mixed_multi:
                    'Kalibrierung für {channelList} geändert. Laden Sie die Oberfläche neu, damit jeder Kanal seine neue Tastgrad-Anzeige und Reglergrenzen übernimmt.',
            },
        },
        calibrationCurve: {
            dialogTitle: 'Kalibrierungskurve',
            loading: 'Kalibrierung wird geladen...',
            notFound: 'Keine Kalibrierungsdaten für diesen Kanal gefunden.',
            loadError: 'Kalibrierungsdaten konnten nicht geladen werden.',
            axisDuty: 'Tastgrad',
            axisRpm: 'RPM',
            legendUp: 'Aufwärts',
            legendDown: 'Abwärts',
            markerStable: 'Stabile Untergrenze',
            curveKindSmooth: 'Gleichmäßig (Abbildung aktiv)',
            curveKindStepped: 'Gestuft (Abbildung deaktiviert)',
            fieldCurveKind: 'Kurve',
            fieldCurveKindTooltip:
                'Wie der Kanal auf Tastgradänderungen reagiert.\nGleichmäßige Lüfter haben eine durchgehende Tastgrad-zu-Drehzahl-Kurve, sodass der Dispatcher den Ziel-Tastgrad durch die Kalibrierung abbildet. Gestufte Lüfter haben diskrete Drehzahlplateaus, daher werden Tastgrade unverändert durchgereicht.',
            fieldRpmMax: 'Spitzen-Drehzahl',
            fieldRpmMaxTooltip:
                'Höchste während des Sweeps beobachtete Drehzahl.\nWird als 100%-Referenz verwendet, wenn ein Ziel-Tastgrad in seinen drehzahlnormalisierten echten Tastgradwert umgerechnet wird.',
            fieldKick: 'Anlaufdauer',
            fieldKickTooltip:
                'Wie lange der Dispatcher den Anlauf-Tastgrad hält, bevor er bei einem Kaltstart auf das Halte-Niveau absenkt.\nGemessen, indem der schlechteste Fall (mit Boost) des Anlauf-Tastgrads des Dispatchers aus dem Stillstand geschrieben wird und gewartet wird, bis sich die Drehzahl in einem stabilen Fenster einpendelt.',
            fieldStart: 'Min. Start-Tastgrad',
            fieldStartTooltip:
                'Niedrigster Tastgrad, der den Lüfter aus dem Stillstand zuverlässig anlaufen lässt.\nUnterhalb dieses Tastgrads beginnt der Lüfter unter Umständen nicht zu drehen, obwohl er weiterdrehen würde, wenn er bereits läuft.',
            fieldSustain: 'Min. Halte-Tastgrad',
            fieldSustainTooltip:
                'Niedrigster Tastgrad, bei dem der Lüfter nach dem Start weiterdreht.\nDer Dispatcher senkt den laufenden Tastgrad nicht unter diesen Wert, es sei denn, der Kanal wird auf 0 gesetzt.',
            fieldStable: 'Min. stabiler Tastgrad',
            fieldStableTooltip:
                'Niedrigster Tastgrad, bei dem der Lüfter ohne Oszillation arbeitet.\nFirmware-gesteuerte Lüfter heben die Drehzahl bei niedrigem Tastgrad über eine interne Untergrenze an und erzeugen so ein hörbares Flattern; der Dispatcher begrenzt das Halte-Niveau nach dem Anlaufschub auf diesen Wert, damit der Lüfter oberhalb des Bandes bleibt.',
            fieldSaturate: 'Tastgrad nahe Plateau',
            fieldSaturateTooltip:
                'Tastgrad, ab dem die Drehzahlzuwächse abnehmen.\nDer Lüfter kann auch oberhalb dieses Tastgrads bis zu 100 % noch einige Umdrehungen zulegen, daher nutzt die Kalibrierung den vollen Tastgradbereich von 0 bis 100 %.',
            fieldTimestamp: 'Kalibriert',
            overridesHeading: 'Überschreibungen',
            fieldKickBoostOverride: 'Anlauf-Boost',
            fieldKickBoostOverrideTooltip:
                'Erzwinge den Kaltstart-Anlaufboost für diesen Kanal an oder aus, oder lass den Daemon anhand der Heuristik der Aufwärtskurve entscheiden.\nDer Boost hebt den Anlauf-Tastgrad kurz über das Halte-Niveau, um den Lüfter über seine Trägheitsschwelle zu drücken.',
            kickBoostAuto: 'Automatisch',
            kickBoostOn: 'Erzwingen ein',
            kickBoostOff: 'Erzwingen aus',
            fieldKickDurationOverride: 'Anlaufdauer überschreiben',
            fieldKickDurationOverrideTooltip:
                'Überschreibt die kalibrierte Anlaufdauer. Leer lassen, um den gemessenen Wert zu verwenden.\nVerlängern, wenn der Lüfter mehr Zeit am Anlauf-Tastgrad benötigt, um sich zu stabilisieren, bevor das Halte-Niveau übernimmt.',
            kickDurationDefault: 'Standard',
            kickDurationReset: 'Auf Standard zurücksetzen',
            kickBoostCurrentlyOn: 'aktuell ein',
            kickBoostCurrentlyOff: 'aktuell aus',
            fieldWalkAfterKick: 'Schrittweises Absenken nach Anlauf',
            fieldWalkAfterKickTooltip:
                'Nach dem Anlauf-Fenster wird der Tastgrad in kleinen Schritten auf das Halte-Niveau abgesenkt. Schützt Lüfter, deren Controller bei einem abrupten Abfall die Stromzufuhr unterbrechen.\nAusschalten, um direkt vom Anlauf zum Halte-Niveau zu springen. Bei den meisten modernen PWM-Lüftern unbedenklich und entfernt die sichtbare Abklingphase nach jedem Kaltstart.',
            overridesSaveFailed: 'Speichern der Kalibrierungs-Überschreibungen fehlgeschlagen',
        },
        deviceExtensionSettings: {
            title: 'Erweiterte Geräteeinstellungen',
            directAccess: 'Direkter Zugriff',
            directAccessDesc:
                'Wenn aktiviert, ignoriert der liquidctl-Treiber den HWMon-Kernel-Treiber\nund kommuniziert direkt mit dem Gerät.\nDies kann bei Geräten hilfreich sein, die Konflikte bei der Verwendung beider Treiber haben.',
            useHwmon: 'HWMon-Treiber verwenden',
            useHwmonDesc:
                'Wechselt den Treiber für dieses Gerät von liquidctl zum HWMon-Kernel-Treiber.\nDies kann die Leistung und Stabilität verbessern, aber möglicherweise verfügbare Funktionen reduzieren.',
            disableDevice: 'liquidctl-Gerät deaktivieren',
            disableInfo:
                'Das Deaktivieren des liquidctl-Treibers deaktiviert dieses Gerät. Ein neues HWMon-basiertes Gerät wird unten im Gerätemenü angezeigt. Sie können das liquidctl-Gerät jederzeit über das Einstellungsmenü wieder aktivieren.',
            commandDelay: 'Befehlsverzögerung',
            commandDelayDesc:
                'Verzögerung in Millisekunden zwischen Befehlen, die an dieses Gerät gesendet werden.\nDies kann bei Geräten helfen, die Kommunikationsprobleme haben,\nwenn mehrere Befehle schnell hintereinander gesendet werden.',
            overdrive: 'GPU Overdrive',
            overdriveDesc:
                'AMD RDNA3/4 GPUs erfordern die Aktivierung von Overdrive für die Lüftersteuerung.\nDies konfiguriert den Kernelparameter amdgpu.ppfeaturemask\nund erfordert einen Systemneustart.',
            overdriveEnable: 'Aktivieren',
            overdriveActive: 'Aktiv',
            overdriveSuccess: 'Overdrive konfiguriert',
            thinkPadFanControl: 'Lüftersteuerung',
            thinkPadFanControlDesc:
                'Aktiviert die ThinkPad ACPI Lüftersteuerung.\nDie Lüftersteuerung ist aus Sicherheitsgründen standardmäßig deaktiviert.\nFortfahren auf eigene Gefahr.',
            thinkPadFullSpeed: 'Volle Geschwindigkeit',
            thinkPadFullSpeedDesc:
                'Aktiviert den Vollgeschwindigkeitsmodus für ThinkPad-Lüfter.\nErmöglicht es den Lüftern bei 100% auf das absolute Maximum zu drehen,\nbetreibt die Lüfter jedoch außerhalb der Spezifikation mit erhöhtem Verschleiß.',
        },
    },
    auth: {
        enterPassword: 'Geben Sie Ihr Passwort ein',
        setNewPassword: 'Geben Sie ein neues Passwort ein',
        changeDefaultPassword:
            'Bitte legen Sie ein Passwort fest, um unbefugten Zugriff zu verhindern. Dieses ist von Ihrem Systemkonto getrennt.',
        accessTokens: 'Zugriffstoken',
        tokenLabel: 'Bezeichnung (z.B. cctv)',
        tokenExpiry: 'Ablaufdatum (optional)',
        createToken: 'Token erstellen',
        tokenCreated: 'Token erstellt',
        tokenCreatedDetail: 'Kopieren Sie diesen Token jetzt. Er wird nicht erneut angezeigt.',
        tokenCopied: 'Token in Zwischenablage kopiert',
        tokenDeleted: 'Token gelöscht',
        tokenCreateError: 'Token konnte nicht erstellt werden',
        tokenDeleteError: 'Token konnte nicht gelöscht werden',
        tokenLoadError: 'Tokens konnten nicht geladen werden',
        tokenDeleteConfirm:
            'Möchten Sie diesen Token wirklich löschen? Alle Dienste, die ihn verwenden, verlieren den Zugriff.',
        tokenDeleteHeader: 'Token löschen',
        noTokens: 'Noch keine Zugriffstoken erstellt.',
        expires: 'Läuft ab',
        expired: 'Abgelaufen',
        active: 'Aktiv',
        never: 'Nie',
        lastUsed: 'Zuletzt verwendet',
        neverUsed: 'Nie verwendet',
        created: 'Erstellt',
        label: 'Bezeichnung',
        actions: 'Aktionen',
        writeAccess: 'Schreibzugriff',
        writeAccessTooltip:
            'Wenn aktiviert, kann dieses Token Änderungen vornehmen. Wenn deaktiviert, kann das Token nur Daten lesen.',
    },
    daemon: {
        status: {
            ok: 'Ok',
            hasWarnings: 'Hat Warnungen',
            hasErrors: 'Hat Fehler',
        },
    },
    // Rendered by the Qt desktop app, which has no translation pipeline of its own.
    // Pushed over IPC and cached there. See shell/qtStrings.ts.
    desktop: {
        closePrompt: {
            title: 'In den Tray minimieren?',
            body: 'Der CoolerControl-Daemon läuft in beiden Fällen im Hintergrund weiter, Ihre Kühlungseinstellungen bleiben also aktiv. Behalten Sie die Oberfläche im Tray für schnellen Zugriff und Desktop-Benachrichtigungen, oder beenden Sie sie ganz.',
            keepInTray: 'Im Tray behalten',
            quit: 'Beenden',
            remember: 'Auswahl merken',
        },
        tray: {
            show: '&Anzeigen',
            hide: '&Ausblenden',
            daemonConnection: '&Daemon-Verbindung…',
            quit: '&Beenden',
            modes: 'Modi',
            sensors: 'Sensoren',
            daemons: 'Daemons',
        },
        cert: {
            title: 'Nicht verifiziertes Daemon-Zertifikat',
            changedTitle: 'Zertifikat geändert',
            // %1 is the daemon host, substituted by Qt via QString::arg.
            body: '%1 verwendet ein selbstsigniertes Zertifikat, das nicht automatisch überprüft werden kann. Fahren Sie nur fort, wenn Sie diesen Daemon kennen.',
            changedBody:
                'Das Zertifikat für %1 ist nicht das zuvor vertraute. Das kann bedeuten, dass der Daemon neu installiert wurde, oder dass die Verbindung abgefangen wird.',
            fingerprint: 'Fingerabdruck (SHA-256):',
            trust: 'Diesem Zertifikat vertrauen',
            cancel: 'Abbrechen',
        },
        wizard: {
            windowTitle: 'Daemon-Verbindungsfehler',
            windowTitleOk: 'Daemon-Verbindung',
            apply: '&Anwenden',
            retry: '&Wiederholen',
            quitApp: '&App beenden',
            introPurpose:
                'Diese Einstellungen legen fest, wie sich die Desktop-App mit dem CoolerControl-Daemon verbindet.',
            introFailed: 'Es konnte keine Verbindung zum CoolerControl-Daemon hergestellt werden.',
            introCheckService:
                'Bitte stellen Sie sicher, dass der systemd-Dienst läuft und verfügbar ist.',
            // %1 is substituted by Qt via QString::arg, not by vue-i18n.
            introDocs: 'Installationsanweisungen finden Sie auf der %1.',
            introDocsLink: 'Dokumentationswebsite',
            introCommands:
                'Einige nützliche Befehle, um den Daemon-Status zu aktivieren und zu prüfen:',
            introCustomAddress:
                'Wenn Sie eine abweichende Adresse für die Verbindung zum Daemon konfiguriert haben, können Sie diese in den folgenden Schritten festlegen:',
            lastError: 'Letzter Fehler:',
            // %1 is substituted by Qt via QString::arg, not by vue-i18n.
            errorNotDaemon:
                'Die Adresse hat geantwortet, aber nicht als CoolerControl Daemon (HTTP %1).',
            errorCertUntrusted: 'Dem Zertifikat des Daemons wurde nicht vertraut.',
            errorCertInvalid:
                'Das Zertifikat des Daemons ist ungültig und die Zertifikatsprüfung ist aktiviert.',
            savedLabel: 'Gespeicherte Verbindung:',
            newConnection: 'Neue Verbindung…',
            removeConnection: 'Entfernen',
            removeConnectionTooltip: 'Den ausgewählten Daemon vergessen.',
            removeConnectionBody: 'Diesen Daemon nicht mehr im Systemabschnitt anbieten?',
            nameLabel: 'Name:',
            nameTooltip: 'Optionale Bezeichnung für diesen Daemon. Leer zeigt Host:Port.',
            addressTitle: 'Daemon-Adresse - Desktop-Anwendung',
            addressSubtitle: 'Passen Sie die Adressfelder nach Bedarf an.',
            hostLabel: 'Host-Adresse:',
            hostTooltip:
                'Die IPv4-, IPv6-Adresse oder der Hostname für die Kommunikation mit dem Daemon.',
            portLabel: 'Port:',
            portTooltip: 'Die Portnummer für die Kommunikation mit dem Daemon.',
            sslTooltip: 'SSL/TLS (HTTPS) aktivieren oder deaktivieren',
            strictTls: 'Zertifikat überprüfen',
            strictTlsTooltip:
                'Ein normal überprüfbares Zertifikat verlangen. Ausgeschaltet lassen, um das selbstsignierte Zertifikat des Daemons zu verwenden, dem bei entfernten Daemons beim ersten Verbinden vertraut wird.',
            defaults: 'Standardwerte',
            defaultsTooltip: 'Die Daemon-Adresse auf die Standardwerte zurücksetzen',
            forgetCerts: 'Vertraute Zertifikate vergessen',
            forgetCertsTooltip:
                'Entfernt die Zertifikate entfernter Daemons, denen diese App vertrauen soll.',
            forgetCertsBody:
                'Diesen Daemon-Zertifikaten wird derzeit vertraut. Wenn Sie sie vergessen, werden Sie bei der nächsten Verbindung erneut um Bestätigung gebeten.',
        },
        versionMismatch: {
            title: 'Versionskonflikt',
            text: 'Die Version der Desktop-App (%1) stimmt nicht mit der Daemon-Version (%2) überein.',
            informative:
                'Bitte starten Sie die Desktop-App neu, um die passende Oberflächenversion zu laden.',
            quitApp: '&App beenden',
            continueAnyway: 'Trotzdem fortfahren',
        },
    },
    device_store: {
        unauthorized: {
            summary: 'Sitzung abgelaufen',
            detail: 'Ihre Sitzung ist abgelaufen. Die Seite wird neu geladen, um sich erneut anzumelden.',
        },
        login: {
            failed: {
                summary: 'Anmeldung fehlgeschlagen',
                detail: 'Ungültiges Passwort',
            },
            rate_limited: {
                summary: 'Anmeldung vorübergehend gesperrt',
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
                overlay: 'Overlay',
            },
            mixFunctionType: {
                min: 'Minimum',
                max: 'Maximum',
                avg: 'Durchschnitt',
                diff: 'Differenz',
                sum: 'Summe',
            },
        },
        customSensor: {
            sensorType: {
                mix: 'Mix',
                file: 'Datei',
                offset: 'Versatz',
                timeAverage: 'Zeitlicher Durchschnitt',
                exponentialMovingAvg: 'Exponentieller gleitender Durchschnitt',
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
        interfaceFont: {
            bundled: 'Mitgeliefert (IBM Plex)',
            system: 'System',
        },
        channelViewType: {
            control: 'Steuerung',
            dashboard: 'Dashboard',
        },
        startupPage: {
            appInfo: 'Info & Werkzeuge',
            homeDashboard: 'Start-Dashboard',
            controls: 'Steuerungen',
        },
        alertState: {
            active: 'Aktiv',
            inactive: 'Inaktiv',
            error: 'Fehler',
        },
        pluginStatus: {
            running: 'Läuft',
            stopped: 'Gestoppt',
            unmanaged: 'Nicht verwaltet',
            disabled: 'Deaktiviert',
        },
        deviceType: {
            customSensors: 'Benutzerdefinierte Sensoren',
            cpu: 'CPU',
            gpu: 'GPU',
            liquidctl: 'Liquidctl',
            hwmon: 'Hwmon',
            servicePlugin: 'Service-Plugin',
        },
        driverType: {
            kernel: 'Kernel',
            liquidctl: 'Liquidctl',
            nvml: 'NVML',
            nvidiaCli: 'Nvidia CLI',
            coolercontrol: 'CoolerControl',
            external: 'Extern',
        },
        lcdModeType: {
            none: 'Keine',
            liquidctl: 'Liquidctl',
            custom: 'Benutzerdefiniert',
        },
        channelType: {
            lcd: 'LCD',
        },
    },
}

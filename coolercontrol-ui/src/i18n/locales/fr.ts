// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

export default {
    common: {
        save: 'Enregistrer',
        mouseActions: 'Actions de la souris',
        moreInfo: "Plus d'informations",
        cancel: 'Annuler',
        add: 'Ajouter',
        yes: 'Oui',
        no: 'Non',
        ok: 'OK',
        error: 'Erreur',
        success: 'Succès',
        loading: 'Chargement...',
        restarting: 'Redémarrage...',
        retry: 'Réessayer',
        saveAndRefresh: 'Enregistrer et actualiser',
        reset: 'Réinitialiser',
        sslTls: 'SSL/TLS',
        protocol: 'Protocole',
        address: 'Adresse',
        port: 'Port',
        search: 'Rechercher',
        finish: 'Terminer',
        next: 'Suivant',
        previous: 'Précédent',
        unmanaged: 'Non géré',
        password: 'Mot de passe',
        currentPassword: 'Mot de passe actuel',
        newPassword: 'Nouveau mot de passe',
        confirmPassword: 'Confirmer le mot de passe',
        savePassword: 'Enregistrer le mot de passe',
        state: 'État',
        name: 'Nom',
        message: 'Message',
        timestamp: 'Horodatage',
        temperature: 'Temp.',
        duty: 'Puissance',
        offset: 'Décalage',
        stay: 'Rester',
        discard: 'Abandonner',
        copy: '(copie)',
        minuteAbbr: 'min',
        rpmAbbr: 'tr/min',
        mhzAbbr: 'MHz',
        ghzAbbr: 'GHz',
        tempUnit: '°C',
        percentUnit: '%',
        secondAbbr: 's',
        wattAbbr: 'W',
        toast: {
            modeCreated: 'Mode Créé',
            modeDuplicated: 'Mode Dupliqué',
            modeNameUpdated: 'Nom du Mode Mis à Jour',
            modeUpdated: 'Mode mis à jour avec les paramètres actuels',
            modeDeleted: 'Mode Supprimé',
            modeActivated: 'Mode Activé',
            customSensorSaved: "Capteur Personnalisé Enregistré et Actualisation de l'UI...",
            customSensorUpdated:
                "Capteur Personnalisé mis à jour avec succès et Actualisation de l'UI...",
            customSensorDeleted:
                "Capteur Personnalisé supprimé avec succès et Actualisation de l'UI...",
            alertSaved: 'Alerte Enregistrée',
            alertUpdated: 'Alerte Mise à Jour',
            alertDeleted: 'Alerte Supprimée',
            alertNotFound: 'Alerte introuvable pour la mise à jour',
            settingsUpdated: "Paramètres mis à jour avec succès et appliqués à l'appareil",
            settingsError:
                "Une erreur s'est produite lors de la tentative d'application de ces paramètres",
            thinkPadFanControlApplied: 'Contrôle du ventilateur ThinkPad appliqué avec succès',
        },
    },
    layout: {
        shell: {
            search: {
                hint: 'Rechercher des appareils, capteurs, paramètres et actions',
                recent: 'Récent',
                jumpTo: 'Aller à',
                noResults: 'Aucun résultat.',
                more: '{count} de plus',
                kindFan: 'Ventilateurs',
                kindSensor: 'Capteurs',
                kindAction: 'Actions',
                kindPage: 'Pages',
            },
            home: 'Accueil',
            cooling: 'Refroidissement',
            monitoring: 'Surveillance',
            devices: 'Appareils',
            settings: 'Paramètres',
            plugins: 'Plugins',
            modes: 'Modes',
            manageModes: 'Gérer les modes',
            access: 'Accès',
            power: 'Alimentation',
            noModes: 'Aucun mode enregistré',
            supportWizards: {
                summary: 'Magiciens du support activés !',
                detail: 'Merci aux bénévoles qui aident nos utilisateurs à faire fonctionner leur matériel et leurs pilotes.',
            },
            coolingPanel: {
                pinned: 'Épinglé',
                pin: 'Épingler',
                unpin: 'Désépingler',
                library: 'Profils et Fonctions',
                profiles: 'Profils',
                functions: 'Fonctions',
                addFolder: 'Ajouter un dossier',
                newFolder: 'Nouveau dossier',
                deleteFolder: 'Supprimer le dossier',
            },
            monitoringPanel: {
                newDashboard: 'Nouveau tableau de bord',
                createAlert: 'Créer une alerte pour ce capteur',
                failAlert: 'Créer une alerte de panne (se déclenche à 0 tr/min)',
                failAlertSuffix: 'Panne',
            },
            devicesPanel: {
                disabled: 'Désactivé',
            },
            sensorDest: {
                monitoring: 'Surveillance',
                cooling: 'Refroidissement',
                lighting: 'Éclairage',
                lcd: 'LCD',
            },
            manageSensors: {
                title: 'Gérer les appareils et les capteurs',
                hint: 'Activez ou désactivez les appareils et capteurs. Il est recommandé de désactiver ceux qui sont inutilisés.',
                pendingChanges:
                    'Aucune modification | {count} modification | {count} modifications',
                applyRestart: 'Appliquer et redémarrer',
                disabledDevices: 'Appareils désactivés',
                openButton: 'Gérer les appareils et les capteurs',
            },
            toast: {
                copy: 'Copier',
                dismissAll: 'Tout ignorer',
            },
            homePanel: {
                overview: 'Aperçu',
                logs: 'Journaux',
            },
            homePage: {
                viewLogs: 'Afficher les journaux',
                logsAll: 'Tous',
                logsWarnings: 'Avertissements+',
                logsErrors: 'Erreurs',
                logsNoMatches: 'Aucune ligne de journal correspondante.',
                getStartedGroup: 'Premiers pas',
                learnGroup: 'Apprendre',
                resourcesGroup: 'Ressources',
                modeAndAlerts: 'Mode et alertes',
                noActiveMode: 'Aucun mode actif',
                setUpCooling: 'Configurer le refroidissement',
            },
            devicesPage: {
                landingHint: 'Sélectionnez un appareil pour afficher ses détails et paramètres.',
                temps: 'températures',
                fans: 'ventilateurs',
                lighting: 'éclairage',
                lcd: 'LCD',
                deviceDisabled: 'Cet appareil est désactivé.',
                enableDevice: "Activer l'appareil",
                disableUnusedSensors: 'Désactiver les capteurs inutilisés… (recommandé)',
                sensors: 'Capteurs',
            },
            hardwareHelp: {
                missingDevice: "Vous attendiez du matériel qui n'apparaît pas ici ?",
            },
            coolingPage: {
                landingHint:
                    'Sélectionnez un ventilateur ou une pompe pour afficher et ajuster son refroidissement.',
                noChannels: "Aucun canal de ventilateur ou de pompe n'a été détecté.",
                noneControllable:
                    'Aucun des canaux de ventilateur ou de pompe détectés ne peut être contrôlé.',
                noticeBlockedByEnvironment:
                    "La détection matérielle n'a pas pu s'exécuter, des canaux de ventilateur et de pompe peuvent donc manquer.",
                fullChart: 'Graphique complet',
                guidedSetup: 'Configuration guidée',
                setupMenu: {
                    autoCreateThisFan: 'Créer automatiquement pour ce ventilateur',
                    createProfile: 'Créer un nouveau profil',
                    calibrateThisFan: 'Étalonner ce ventilateur',
                    autoCreateAllFans: 'Créer automatiquement pour tous les ventilateurs',
                    calibrateAllFans: 'Étalonner tous les ventilateurs',
                },
                manualAt: 'Manuel {duty} %',
                manualDuty: 'Cycle manuel',
                modeProfile: 'Profil',
                modeManual: 'Manuel',
                modeUnmanaged: 'Non géré',
                unmanagedHint:
                    "L'appareil ou son firmware contrôle ce canal. CoolerControl n'enverra aucune commande de vitesse.",
                apply: 'Appliquer',
                saveAndApply: 'Enregistrer et appliquer',
                unsavedChanges: "Des modifications de ce canal n'ont pas été appliquées.",
                unsavedChangesHeader: 'Modifications non enregistrées',
                selectProfile: 'Sélectionner un profil',
                sharedWith: 'Partagé avec {count} autres',
                sharedTooltip: "Ce profil pilote également d'autres canaux.",
                notShared: 'Ce ventilateur uniquement',
                notSharedTooltip: 'Ce profil ne pilote que ce canal.',
                forkForFan: 'Dupliquer pour ce ventilateur',
                forkQualifier: 'copie de {channel}',
                fork: {
                    confirmHeader: 'Dupliquer pour ce ventilateur',
                    confirmMessage:
                        "Copier le profil « {profile} » vers un nouveau profil « {copy} » et l'affecter à {channel}.\n\nL'original reste intact : les modifications ici n'affecteront que {channel}.",
                    accept: 'Créer une copie',
                },
                convert: {
                    button: 'Convertir pour la calibration',
                    tooltip:
                        'Ce ventilateur est calibré : ses vitesses enregistrées sont désormais lues comme des vitesses réelles et reconverties à chaque écriture. Convertissez-les pour que le ventilateur se comporte comme avant la calibration.',
                    confirmHeader: 'Convertir pour le ventilateur calibré',
                    confirmProfile:
                        "Copier le profil « {profile} » vers un nouveau profil « {copy} », convertir ses vitesses et l'affecter à {channel}.\n\nNe convertissez que des vitesses définies avant la calibration de ce ventilateur. Une double conversion fait tourner le ventilateur à la mauvaise vitesse. L'original reste intact.",
                    confirmManual:
                        "Convertir le rapport cyclique manuel de {channel} pour que le ventilateur conserve la vitesse qu'il avait avant la calibration.\n\nNe convertissez qu'une valeur définie avant la calibration de ce ventilateur. Une double conversion fait tourner le ventilateur à la mauvaise vitesse.",
                    nameQualifier: 'étalonné',
                    accept: 'Convertir',
                    successProfile:
                        '« {profile} » a été affecté à {channel} avec les vitesses converties.',
                    successManual: 'Rapport cyclique manuel converti à {duty} %.',
                    error: 'Impossible de convertir les vitesses de ce ventilateur.',
                    floorHeading: 'Certains points sont passés à 0 %',
                    floorNotice:
                        '{count} point(s) se situaient sous la vitesse la plus basse réglable sur {channel} après la calibration : ils sont passés à 0 %. Vérifiez la nouvelle courbe avant de vous y fier.',
                    modesHeading: "Les modes utilisent toujours l'original",
                    modesReminder:
                        "Ces modes affectent encore le profil d'origine à {channel} : {modes}. Mettez-les à jour pour utiliser la copie convertie.",
                },
                notControllable:
                    'Ce canal signale sa vitesse mais ne peut pas être contrôlé par CoolerControl.',
                verdictFirmwareOverride:
                    "CoolerControl a réglé ce canal en commande manuelle, mais le firmware l'a rétabli.",
                verdictFamilyMayNeedOutOfTree:
                    "Aucune commande de ventilateur inscriptible n'a été trouvée pour ce canal. Sur cette famille de puces, un autre pilote noyau la fournit parfois.",
                verdictNotSupportedByDriver:
                    'Le pilote utilisé ne propose aucune commande de ventilateur pour ce canal.',
                verdictNoPwm:
                    'Le pilote chargé ne propose aucune commande de ventilateur pour ce canal, seulement sa vitesse.',
                verdictPwmReadOnly:
                    'Le pilote chargé propose une commande de ventilateur pour ce canal, mais la marque en lecture seule.',
                verdictIgnoresDuty:
                    "Ce canal a accepté les changements de puissance, mais sa vitesse mesurée n'a jamais réagi.",
                verdictUnverifiable:
                    "Ce canal n'a pas de tachymètre exploitable, sa réaction aux changements de puissance ne peut donc pas être vérifiée.",
                verdictEvidenceLabel: 'Mesuré sur cette machine :',
                evidenceNoPwmFile: 'aucune commande de ventilateur exposée',
                evidencePwmNotWritable: 'la commande de ventilateur est en lecture seule',
                evidenceHasTachometer: 'lecture de vitesse disponible',
                evidenceNoTachometer: 'aucune lecture de vitesse',
                verdictLearnMore: 'Que puis-je y faire ?',
                verdictFoundSomethingThatWorks: 'Vous avez trouvé une solution ? Dites-le nous',
                activeMode: 'Actif',
                previousMode: 'Précédent',
                activate: 'Activer',
                noModes:
                    'Aucun mode enregistré pour le moment. Les modes capturent tous les paramètres des canaux pour un basculement rapide.',
                powerProfiles: {
                    title: "Profil d'alimentation du systeme",
                    description:
                        "Activer automatiquement un mode lorsque le profil d'alimentation du systeme change.",
                    activeProfile: 'Profil actuel : {profile}',
                    noMode: 'Aucun mode',
                    saveFailed:
                        "L'association des profils d'alimentation n'a pas pu etre enregistree.",
                    profileNames: {
                        'power-saver': "Economie d'energie",
                        balanced: 'Equilibre',
                        performance: 'Performance',
                    },
                },
                miniCurveHint:
                    'Courbe du profil assigné. Le point marque la cible à la température actuelle de la source ; la Fonction du canal détermine la valeur réelle.',
                chain: {
                    tempSource: 'Source de température',
                    profile: 'Profil',
                    function: 'Fonction',
                },
            },
        },
        topbar: {
            login: 'Connexion',
            logout: 'Déconnexion',
            changePassword: 'Changer de mot de passe',
            accessTokens: "Jetons d'accès",
            restartUI: "Redémarrer l'interface",
            restartDaemonAndUI: "Redémarrer le daemon et l'interface",
            restartConfirmMessage: "Êtes-vous sûr de vouloir redémarrer le daemon et l'interface ?",
            restartConfirmHeader: 'Redémarrage du daemon',
            shutdownSuccess: "Signal d'arrêt du daemon accepté",
            shutdownError:
                "Erreur inconnue lors de l'envoi du signal d'arrêt. Consultez les journaux pour plus de détails.",
            quitDesktopApp: "Quitter l'application",
            back: 'Retour',
            expandMenu: 'Développer le menu',
            collapseMenu: 'Réduire le menu',
            alerts: 'Alertes',
            settings: 'Paramètres',
            openInBrowser: 'Ouvrir dans le navigateur',
            loginSuccessful: 'Connexion réussie',
        },
        settings: {
            title: 'Paramètres',
            devices: {
                toggleRequiresRestart:
                    "La modification des appareils ou des capteurs nécessite un redémarrage du daemon et de l'interface. Voulez-vous le faire maintenant ?",
                enableDevices: 'Activer les appareils',
                unknownError:
                    "Erreur inconnue lors de la tentative d'application des modifications à tous les appareils. Consultez les journaux pour plus de détails.",
            },
            plugins: {
                privileged: 'Accès privilégié',
                pluginUrl: "Page d'accueil",
                restricted: 'Accès restreint',
                settingsSaved: 'Paramètres du plugin enregistrés avec succès',
                settingsNotSaved: "Échec de l'enregistrement des paramètres du plugin",
            },
            appearance: 'Apparence',
            general: 'Général',
            language: 'Langue',
            selectLanguage: 'Sélectionner la langue',
            systemLanguage: 'Système',
            fullScreen: 'Plein écran',
            railToCollapse: 'Barre de navigation pour réduire',
            eyeCandy: 'Effets visuels',
            interfaceFont: "Police de l'interface",
            introduction: 'Introduction',
            startTour: 'Démarrer la visite',
            timeFormat: "Format de l'heure",
            time24h: '24 heures',
            time12h: '12 heures',
            frequencyPrecision: 'Précision de la fréquence',
            startupPage: 'Page de démarrage',
            dashboardLineSize: 'Taille des lignes du tableau de bord',
            themeStyle: 'Style du thème',
            themeGroups: {
                builtIn: 'Intégrés',
                installed: 'Installés',
                custom: 'Personnalisé',
            },
            desktop: 'Bureau',
            startInTray: "Démarrer dans la barre d'état",
            closeToTray: "Réduire dans la barre d'état",
            zoom: 'Zoom',
            desktopStartupDelay: 'Délai de démarrage du bureau',
            groups: {
                startup: 'Démarrage',
                performance: 'Performances',
                devices: 'Périphériques et détection',
                liquidctl: 'Liquidctl',
            },
            applySettingsOnStartup: 'Appliquer les paramètres au démarrage',
            deviceDelayAtStartup:
                "Délai avant de commencer la communication de l'appareil (en secondes).\nAide avec les appareils qui prennent du temps à s'initialiser ou sont détectés de manière intermittente",
            pollingRate:
                "Le taux auquel les données du capteur sont interrogées (en secondes).\nUn taux d'interrogation plus élevé réduira l'utilisation des ressources, et un taux plus bas augmentera la réactivité.\nUn taux inférieur à 1,0 doit être utilisé avec précaution.",
            compressApiPayload:
                "Activer la compression de la réponse pour réduire la taille de la charge utile de l'API,\nmais notez que cela augmentera l'utilisation du CPU.",
            liquidctlIntegration:
                "Désactiver cela désactivera complètement l'intégration de Liquidctl,\nindépendamment de l'état d'installation du package coolercontrol-liqctld. Si disponible, les pilotes HWMon seront utilisés à la place.",
            liquidctlDeviceInit:
                "Attention : Désactivez cela UNIQUEMENT si vous, ou un autre programme, gérez l'initialisation de l'appareil liquidctl. Cela peut aider à éviter les conflits avec d'autres programmes.",
            hideDuplicateDevices: 'Masquer les appareils en double',
            drivePowerState: "État d'alimentation du disque ",
            sensorsAutoDetect: 'Détection auto des capteurs',
            sensorsConfig: 'Configuration lm-sensors',
            deviceListener: "Surveillance des changements d'appareils",
            customTheme: {
                title: 'Thème Personnalisé',
                accent: "Couleur d'Accent",
                accentGradientTo: "Fin du dégradé d'accentuation",
                bgOne: 'Fond Principal',
                bgTwo: 'Fond Secondaire',
                border: 'Couleur de la Bordure',
                text: 'Couleur du Texte',
                textSecondary: 'Couleur du Texte Secondaire',
                success: 'Succès',
                warning: 'Avertissement',
                error: 'Erreur',
                info: 'Info',
                export: 'Exporter le Thème',
                import: 'Importer le Thème',
                copyCode: 'Copier le Code',
                pasteCode: 'Coller le Code',
                themeCodeCopied: 'Code du thème copié',
                themeApplied: 'Thème appliqué',
                invalidThemeCode: 'Code de thème invalide',
            },
            tooltips: {
                timeFormat: "Format de l'heure : 12 heures (AM/PM) ou 24 heures",
                frequencyPrecision: 'Ajuster la précision des valeurs de fréquence affichées.',
                startupPage: "La page affichée après le chargement de l'application.",
                railToCollapse:
                    'Utiliser aussi la zone vide de la barre de navigation pour étendre ou réduire le menu.',
                eyeCandy:
                    'Activer les animations visuelles comme les icônes de ventilateurs en rotation.\nCela utilisera des ressources GPU supplémentaires.',
                interfaceFont:
                    'Utiliser les polices fournies avec CoolerControl ou celles configurées sur votre système.',
                fullScreen: 'Basculer en mode plein écran',
                lineThickness:
                    "Ajuster l'épaisseur des lignes des graphiques sur le tableau de bord",
                startInTray:
                    "Au démarrage, la fenêtre principale de l'interface utilisateur sera masquée et seul\nle symbole de la barre d'état système sera visible.",
                closeToTray:
                    "Fermer la fenêtre de l'application laissera l'application en cours d'exécution dans la barre d'état système",
                zoom: "Définir manuellement le niveau de zoom de l'interface utilisateur.",
                desktopStartupDelay:
                    "Ajoute un délai avant de démarrer l'application de bureau (en secondes).\nAide à résoudre les problèmes qui surviennent lorsque l'application de bureau\nest démarrée automatiquement à la connexion ou démarre trop rapidement",
                unlockRange: 'Autoriser les valeurs hors de la plage recommandée',
                lockRange: 'Limiter à la plage recommandée',
                applySettingsOnStartup:
                    'Appliquer automatiquement les paramètres au démarrage du daemon et lors de la sortie de veille',
                deviceDelayAtStartup:
                    "Délai avant de commencer la communication de l'appareil (en secondes).\nAide avec les appareils qui prennent du temps à s'initialiser ou sont détectés de manière intermittente",
                pollingRate:
                    "Le taux auquel les données du capteur sont interrogées (en secondes).\nUn taux d'interrogation plus élevé réduira l'utilisation des ressources, et un taux plus bas augmentera la réactivité.\nUn taux inférieur à 1,0 doit être utilisé avec précaution.",
                compressApiPayload: "Activer la compression de la charge utile de l'API",
                liquidctlIntegration:
                    "Désactiver cela désactivera complètement l'intégration de Liquidctl,\nindépendamment de l'état d'installation du package coolercontrol-liqctld. Si disponible, les pilotes HWMon seront utilisés à la place.",
                liquidctlDeviceInit:
                    "Attention : Désactivez cela UNIQUEMENT si vous, ou un autre programme, gérez l'initialisation de l'appareil liquidctl.\nCela peut aider à éviter les conflits avec d'autres programmes.",
                hideDuplicateDevices:
                    "Certains appareils sont pris en charge à la fois par les pilotes Liquidctl et HWMon. Liquidctl est utilisé par défaut pour ses fonctionnalités supplémentaires. Pour utiliser les pilotes HWMon à la place, désactivez cela et l'appareil liquidctl pour éviter les conflits de pilotes.",
                drivePowerState:
                    "Les SSD et les HDD en particulier peuvent s'arrêter et entrer dans un état de faible consommation d'énergie.\nCette option, lorsqu'elle est activée et que le disque la prend en charge, rapportera les températures du disque\ncomme 0°C lorsqu'il est arrêté afin que les profils de ventilateur puissent être ajustés en conséquence.",
                sensorsAutoDetect:
                    'Détecter automatiquement les capteurs matériels Super-I/O et charger\nles modules noyau au démarrage. (x86_64 uniquement)',
                sensorsConfig:
                    'Utiliser les noms de capteurs et les capteurs masqués des fichiers\nde configuration lm-sensors (/etc/sensors3.conf et /etc/sensors.d).\nLes noms définis dans CoolerControl sont toujours prioritaires.',
                deviceListener:
                    "Surveiller les événements d'ajout/suppression d'appareils (ex. branchement USB)\net notifier lorsque des changements matériels sont détectés.",
                triggersDaemonRestart: 'Déclenche un redémarrage automatique du daemon',
                copyThemeCode:
                    'Copier un code compact représentant votre thème personnalisé actuel.\nPartagez-le dans des chats ou forums.',
                pasteThemeCode:
                    "Appliquer un thème personnalisé depuis un code (cct1:...) qu'on vous a partagé.",
            },
            applySettingAndRestart:
                "Changer ce paramètre nécessite un redémarrage du daemon et de l'interface utilisateur. Êtes-vous sûr de vouloir le faire maintenant?",
            restartHeader: 'Appliquer le paramètre et redémarrer',
            success: 'Succès',
            successDetail: 'Opération terminée avec succès',
            languageChangeConfirm: 'Changer de langue ?',
            languageChangeConfirmMessage:
                "Êtes-vous sûr de vouloir continuer ? Si certains éléments de l'interface ne s'affichent pas correctement, veuillez actualiser la page manuellement.",
            languageChangeSuccess: 'Langue changée avec succès.',
            languageChangeError: 'Échec du changement de langue. Veuillez réessayer.',
            themeChangeSuccess: 'Thème changé avec succès.',
        },
        menu: {
            dashboards: 'Tableaux de bord',
            customSensors: 'Capteurs personnalisés',
            alerts: 'Alertes',
            pinned: 'Épinglé',
            tooltips: {
                createMode: 'Créer un mode à partir des paramètres actuels',
                addProfile: 'Ajouter un profil',
                addAlert: 'Ajouter une alerte',
                addDashboard: 'Ajouter un tableau de bord',
                duplicate: 'Dupliquer',
                rename: 'Renommer',
                addCustomSensor: 'Ajouter un capteur personnalisé',
                addFunction: 'Ajouter une fonction',
                chooseColor: 'Choisir une couleur',
            },
        },
        plugins: {
            plugins: 'Plugins',
            notFound: 'Plugin introuvable',
            type: 'Type',
            address: 'Adresse',
            privileges: 'Privilèges',
            url: 'URL',
            start: 'Démarrer',
            stop: 'Arrêter',
            restart: 'Redémarrer',
            started: 'Plugin démarré',
            stopped: 'Plugin arrêté',
            restarted: 'Plugin redémarré',
            startFailed: 'Échec du démarrage du plugin',
            stopFailed: "Échec de l'arrêt du plugin",
            restartFailed: 'Échec du redémarrage du plugin',
            overview: 'Aperçu des Plugins',
            gettingStarted:
                "Les plugins étendent CoolerControl avec une prise en charge supplémentaire des appareils, des intégrations et de l'automatisation. Ils peuvent fournir de nouveaux capteurs et commandes d'appareils, se connecter à des services externes ou ajouter des pages d'interface personnalisées.",
            findPlugins: 'Trouver et installer des Plugins',
            restartNote:
                "Si vous avez récemment ajouté un nouveau plugin et qu'il n'apparaît pas ici, redémarrez le démon CoolerControl.",
            containerNote:
                "Lorsque CoolerControl s'exécute dans un conteneur, les plugins doivent être placés dans le dossier partagé virtuel persistant afin qu'ils survivent aux redémarrages du conteneur.",
            installedPlugins: 'Plugins Installés',
            noPlugins: 'Aucun plugin installé',
            info: 'Info',
            description: 'Description',
            enable: 'Activer',
            disable: 'Désactiver',
            pluginDisabled: 'Plugin désactivé.',
            pluginEnabled: 'Plugin activé.',
            pluginDisabledRestart: 'Plugin désactivé. Redémarrez le daemon pour appliquer.',
            pluginEnabledRestart: 'Plugin activé. Redémarrez le daemon pour appliquer.',
            disableFailed: 'Impossible de désactiver le plugin',
            enableFailed: "Impossible d'activer le plugin",
            serviceLogs: 'Journaux du service',
            commandCopied: 'Commande copiee dans le presse-papiers',
        },
        add: {
            profile: 'Profil',
            function: 'Fonction',
            customSensor: 'Capteur personnalisé',
        },
    },
    views: {
        daemon: {
            title: 'Daemon',
            daemonErrors: 'Erreurs du Daemon',
            daemonErrorsDetail:
                'Le daemon a signalé des erreurs. Consultez les journaux pour plus de détails.',
            daemonDisconnected: 'Daemon Déconnecté',
            daemonDisconnectedDetail:
                "Impossible de se connecter au daemon. Veuillez vérifier si le daemon est en cours d'exécution.",
            connectionRestored: 'Connexion Rétablie',
            connectionRestoredMessage: 'La connexion au daemon a été rétablie.',
            reconnecting: 'Reconnexion en cours...',
            disconnectedFor: 'Déconnecté depuis {time}',
        },
        speed: {
            applySetting: 'Appliquer le Paramètre',
        },
        customSensors: {
            missingSourcesNotice:
                'Les sources de température suivantes ne sont plus présentes et seront supprimées lors de la sauvegarde: {sources}',
            sensorType: 'Type de Capteur',
            mixFunction: 'Fonction de Mélange',
            howCalculateValue: 'Comment calculer la valeur résultante du capteur',
            tempFile: 'Fichier de Température',
            filePathTooltip:
                'Entrez le chemin absolu vers le fichier de température à utiliser pour ce capteur.\nLe fichier doit utiliser le format de données sysfs standard :\nUn nombre à virgule fixe en millidegrés Celsius.\np. ex. 80000 pour 80°C.\nLe fichier est vérifié lors de la soumission.',
            browse: 'Parcourir',
            browseCustomSensorFile: 'Parcourir pour un fichier de capteur personnalisé',
            tempSources: 'Sources de Température',
            tempSource: 'Source de Température',
            tempSourcesTooltip:
                'Sources de température à utiliser dans la fonction de mélange<br/><i>Remarque : lors de la combinaison de plusieurs capteurs personnalisés, seules les relations directes parent-enfant sont autorisées.<br/>Utilisez des Profils de Mélange pour des configurations plus complexes.</i>',
            offset: 'Décalage',
            offsetTooltip:
                'Saisissez un décalage négatif ou positif à appliquer au capteur source.<br/><i>Remarque : la valeur finale est limitée aux plages de température normales.</i>',
            timeWindow: 'Fenêtre de Lissage',
            timeWindowTooltip:
                'Nombre de secondes des échantillons récents à lisser ensemble.<br/><i>Remarque : doit être compris entre 1 et 300 secondes.</i>',
            helpText: {
                mix: 'Combine plusieurs sources de température via la fonction choisie (Min/Max/Moyenne/Delta/Moyenne Pondérée). À utiliser pour piloter les ventilateurs depuis le plus chaud de plusieurs capteurs, ou pour équilibrer entre les zones.',
                file: 'Lit la température depuis un chemin de fichier. À utiliser pour les capteurs non détectés automatiquement par CoolerControl.',
                offset: "Ajoute ou soustrait une valeur fixe d'une source de température. À utiliser pour calibrer une imprécision connue du capteur.",
                timeAverage:
                    "Moyenne arithmétique sur une fenêtre temporelle fixe. La sortie est bornée par la plage d'entrée et ne dépasse jamais. Pour les ventilateurs qui doivent ignorer les pics de température brefs.",
                exponentialMovingAvg:
                    'Moyenne pondérée favorisant les lectures récentes. Plus lisse que la Moyenne Temporelle pour la même fenêtre, mais nécessite environ 3 fois la longueur de la fenêtre pour suivre complètement un changement durable. Pour les ventilateurs qui doivent suivre les vraies tendances sans gigue.',
            },
            tempWeights: 'Poids des Températures',
            tempName: 'Nom de la Température',
            weight: 'Poids',
            saveCustomSensor: 'Enregistrer le Capteur Personnalisé',
            unsavedChanges:
                'Il y a des changements non enregistrés apportés à ce Capteur Personnalisé.',
            unsavedChangesHeader: 'Changements non enregistrés',
            selectCustomSensorFile: 'Sélectionner un Fichier de Capteur Personnalisé',
            deleteCustomSensor: 'Supprimer le Capteur Personnalisé',
            deleteCustomSensorConfirm:
                'Êtes-vous sûr de vouloir supprimer le capteur personnalisé : "{name}" ?',
        },
        dashboard: {
            timeRange: 'Plage de Temps',
            chartType: 'Type de Graphique',
            filterSensors: 'Filtrer les Capteurs',
            mouseActions:
                "Actions de la souris sur le tableau de bord :\n- Mettre en surbrillance la sélection pour zoomer.\n- Ctrl+Défilement pour zoomer.\n- Cliquer avec le bouton droit pour faire glisser lorsque zoomé.\n- Double-cliquer pour réinitialiser et reprendre la mise à jour.\n- Ctrl+cliquer ou cliquer avec le bouton du milieu pour afficher tous les capteurs dans l'info-bulle.",
            fullPage: 'Pleine Page',
            filterTags: 'Filtrer les Tags',
            filterByTag: 'Filtrer par Tag',
            filterBySensor: 'Filtrer par Capteur',
            filterTypes: 'Filtrer les Types',
            filterByDataType: 'Filtrer par Type de Données',
            exitFullPage: 'Quitter la Pleine Page',
            deleteDashboard: 'Supprimer le Tableau de Bord',
            deleteDashboardConfirm:
                'Êtes-vous sûr de vouloir supprimer le tableau de bord : "{name}" ?',
            setAsHome: 'Définir comme Accueil',
            duplicateDashboard: 'Dupliquer le Tableau de Bord',
            openCooling: 'Ouvrir les contrôles de refroidissement',
            editCustomSensor: 'Modifier le capteur personnalisé',
            openCustomSensor: 'Ouvrir les paramètres du capteur personnalisé',
        },
        appInfo: {
            noWarranty: 'Ce programme est fourni sans absolument aucune garantie.',
            changeStartupPage: 'Modifier la page de démarrage dans les paramètres',
            daemonStatus: 'État du Daemon',
            acknowledgeIssues: 'Reconnaître les Problèmes',
            status: 'État',
            host: 'Hôte',
            uptime: 'Temps de Fonctionnement',
            version: 'Version',
            processId: 'ID de Processus',
            memoryUsage: 'Utilisation de la Mémoire',
            liquidctl: 'Liquidctl',
            connected: 'Connecté',
            disconnected: 'Déconnecté',
            helpfulLinks: 'Liens Utiles',
            uiTour: "Visite de l'interface",
            gettingStarted: 'Premiers Pas',
            helpSettingUp: 'Aide à la configuration du contrôle des ventilateurs',
            gettingStartedStep1: 'Ouvrez Refroidissement et choisissez le ventilateur à contrôler.',
            gettingStartedStep2:
                'Choisissez Configuration guidée, puis Nouveau Profil, pour définir sa courbe de ventilation.',
            gettingStartedStep3:
                'Réutilisez ce profil sur autant de ventilateurs que vous le souhaitez.',
            gettingStartedAutoCreate:
                '{wizard} permet de configurer des profils de base pour tous vos ventilateurs en une seule fois.',
            gettingStartedAutoCreateLink: 'Créer des profils automatiquement',
            calibrateFansLink: 'étalonnez vos ventilateurs',
            hardwareSupport: 'Support Matériel',
            whatsNew: 'Nouveautés',
            logsAndDiagnostics: 'Journaux et Diagnostics',
            downloadCurrentLog: 'Télécharger le Journal Actuel',
            deviceHealth: 'État des Périphériques',
            deviceHealthOk: 'Tous les capteurs et canaux fonctionnent correctement.',
            detection: 'Détection des puces',
            detectionDescription:
                "Ce que la détection de puce Super-I/O a trouvé au démarrage du démon. Les modules sont chargés au démarrage, c'est donc cette exécution qui explique une puce non prise en charge.",
            detectionButton: 'Détection des puces',
            detectionNotRun:
                "Aucune détection n'a été exécutée, rien n'est donc connu des puces Super-I/O de cette machine.",
            detectionSecureBoot: 'Secure Boot',
            detectionContainer: 'Conteneur',
            detectionDevPort: '/dev/port disponible',
            detectionChips: 'Puces détectées',
            detectionNoChips: "Aucune puce Super-I/O n'a été détectée.",
            detectionBlacklisted: 'Pilotes sur liste noire',
            hardwareSupportOk: 'Tout le matériel détecté est pris en charge et contrôlable.',
            hardwareReport: 'Rapport matériel',
            hardwareReportDescription:
                "Un résumé de ce que CoolerControl voit sur cette machine, prêt à coller dans un canal d'assistance. Les numéros de série et identifiants sont exclus.",
            hardwareReportFull: "Inclure l'arborescence hwmon complète",
            hardwareReportEmpty: "Le rapport n'a pas pu être généré.",
            hardwareReportButton: 'Rapport matériel',
            hardwareReportCopy: 'Copier',
            hardwareReportCopied: 'Copié',
            findingNoDriverBound: 'Une puce a été détectée, mais aucun pilote chargé ne la gère.',
            findingBlacklisted: "Ce pilote est sur liste noire et n'a pas été chargé.",
            findingBlockedByEnvironment:
                "La détection matérielle n'a pas pu s'exécuter dans cet environnement.",
            findingBlockedBySecureBoot:
                "La détection matérielle n'a pas pu s'exécuter car le démarrage sécurisé est activé.",
            findingBlockedByContainer:
                "La détection matérielle n'a pas pu s'exécuter dans un conteneur.",
            findingBlockedByNoDevPort:
                "La détection matérielle n'a pas pu s'exécuter car /dev/port n'est pas disponible.",
            findingDetectionUnsupported:
                "La détection matérielle n'est pas prise en charge sur cette architecture.",
            failsafeActive: 'Valeurs de secours utilisées',
            deviceUnreachable: 'Le périphérique ne répond pas',
            deviceUnreachableDetail:
                'Le pilote ne répond plus, ce périphérique ne peut donc être ni lu ni contrôlé. Nouvelle tentative périodique.',
            missingTempSource: 'Source de température manquante',
            staleTempSource: 'La source de température utilise des valeurs de secours',
            stressTest: 'Tests de stress thermique',
            stressTestTooltip:
                'Génère une charge thermique soutenue pour valider\nles courbes de ventilateur et les profils de refroidissement.\nLes résultats peuvent varier selon le matériel.\nInstallez stress-ng pour des backends supplémentaires.',
            cpuStress: 'Stress CPU',
            gpuStress: 'Stress GPU',
            gpuStressTooltip:
                "Peut nécessiter des pilotes Vulkan ou OpenGL ES<br>lors de l'utilisation du backend intégré.",
            ramStress: 'Stress RAM',
            driveStress: 'Stress disque',
            driveStressTooltip:
                "Stress d'E/S sur un périphérique bloc pour générer<br>de la chaleur sur les contrôleurs de disque.<br>stress-ng nécessite que le périphérique soit monté.",
            builtInBackend: 'intégré',
            stressNgBackend: 'stress-ng',
            backendTooltip:
                "Choisissez le backend du test de stress.<br>Le backend intégré fonctionne sans dépendances externes.<br>stress-ng (lorsqu'il est installé) fournit des variantes de stresseurs supplémentaires.",
            selectDrive: 'Sélectionner un disque',
            selectGpu: 'Sélectionner un GPU',
            allGpus: 'Tous les GPU',
            start: 'Démarrer',
            stop: 'Arrêter',
            stopAll: 'Tout arrêter',
            active: 'Actif',
            inactive: 'Inactif',
            psuWarningHeader: 'Avertissement: consommation élevée',
            psuWarningMessage:
                "L'exécution simultanée des tests de stress CPU et GPU sollicitera fortement votre alimentation. En cas d'overclocking ou avec une alimentation de faible puissance, une instabilité système peut survenir. Voulez-vous continuer ?",
            proceed: 'Continuer',
        },
        alerts: {
            triggersOutside: 'se déclenche en dessous de {min} ou au-dessus de {max}{unit}',
            triggersAbove: 'se déclenche au-dessus de {max}{unit}',
            stateSince: '{state} depuis {time}',
            deleteAlert: "Supprimer l'Alerte",
            duplicateAlert: "Dupliquer l'Alerte",
            alertsOverview: 'Aperçu des Alertes',
            alertLogs: "Journaux d'Alertes",
            alertTriggered: 'Alerte Déclenchée',
            alertRecovered: 'Alerte Récupérée',
            alertError: "Erreur d'alerte",
            alertSensorsReadable: "Capteurs d'alerte de nouveau lisibles",
            deleteAlertConfirm: 'Êtes-vous sûr de vouloir supprimer : "{name}" ?',
            saveAlert: "Enregistrer l'Alerte",
            channelSources: "Sources de Canal pour l'Alerte",
            channelSourcesTooltip:
                'Les sources de canal surveillées par cette Alerte.\nUn type de capteur par Alerte : la première sélection filtre les autres.',
            triggerConditions: 'Conditions de Déclenchement',
            maxValueTooltip: "Les valeurs au-dessus de ceci déclencheront l'alerte.",
            minValueTooltip: "Les valeurs en dessous de ceci déclencheront l'alerte.",
            warmupDurationTooltip:
                "Durée pendant laquelle une condition doit être active avant que l'alerte soit considérée comme active.\nCette durée est vérifiée uniquement à intervalles réguliers\net peut donc varier.",
            cooldownDurationTooltip:
                "Durée pendant laquelle la valeur doit rester dans la plage avant que l'alerte récupère.\nÉvite les allers-retours rapides entre déclenchée et résolue.",
            cooldownLessThan: 'condition récupérée plus longtemps que',
            repeatInterval: 'Répéter la notification toutes les',
            repeatIntervalTooltip:
                "Renvoyer la notification de bureau à cet intervalle tant que l'alerte reste active.\n0 désactive les notifications répétées.",
            enabled: 'Activée',
            enabledTooltip: "Une alerte désactivée n'est pas évaluée du tout.",
            sectionGeneral: 'Général',
            sectionNotifications: 'Notifications',
            sectionActions: 'Actions',
            silence: 'Mettre en sourdine',
            silenceTooltip:
                "Mettre en sourdine : supprime les notifications et l'arrêt pendant un moment.\nL'alerte continue d'être évaluée et affiche son état.",
            silence15m: 'Sourdine pendant 15 minutes',
            silence1h: 'Sourdine pendant 1 heure',
            silence8h: 'Sourdine pendant 8 heures',
            silence24h: 'Sourdine pendant 24 heures',
            unsilence: 'Désactiver la sourdine maintenant',
            enableAlert: "Activer l'Alerte",
            disableAlert: "Désactiver l'Alerte",
            silencedUntil: "En sourdine jusqu'à {time}",
            disabledLabel: 'Désactivée',
            greaterThan: 'supérieur à',
            lessThan: 'inférieur à',
            newAlert: 'Nouvelle Alerte',
            warmupGreaterThan: 'condition déclenchée plus longtemps que',
            unsavedChanges: 'Il y a des changements non enregistrés apportés à cette Alerte.',
            unsavedChangesHeader: 'Changements non enregistrés',
            desktopNotify: 'notification de bureau',
            desktopNotifyTooltip:
                "Activer les notifications de bureau lorsque l'alerte est déclenchée.\n(Si pris en charge)",
            desktopNotifyRecovery: 'notification de bureau lors de la récupération',
            desktopNotifyRecoveryTooltip:
                "Activer les notifications de bureau lorsque l'alerte récupère.\n(Si pris en charge)",
            desktopNotifyAudio: 'audio de notification de bureau',
            desktopNotifyAudioTooltip:
                "Activer l'audio de notification de bureau lorsque l'alerte est déclenchée.\n(Si pris en charge)",
            shutdownOnActivation: "arrêt lors de l'activation",
            shutdownOnActivationTooltip:
                "Activer l'arrêt du système lorsque l'alerte est déclenchée.\nL'arrêt du système commencera une minute après le déclenchement de l'alerte\net sera annulé si l'alerte récupère.",
        },
        profiles: {
            targetDuty: 'Cible',
            actualDuty: 'Réel',
            targetHint:
                'La cible est calculée à partir des températures actuelles, avant l’application de la Fonction du canal. Le lissage et l’hystérésis peuvent faire différer la valeur réelle.',
            createProfile: 'Créer un profil',
            deleteProfile: 'Supprimer le profil',
            profileType: 'Type de profil',
            fixedDuty: 'Vitesse de ventilateur fixe',
            tempSource: 'Source de température',
            memberProfiles: 'Profils membres',
            mixFunction: 'Fonction de mixage',
            applyMixFunction: 'Appliquer la fonction de mixage aux profils sélectionnés',
            profilesToMix: 'Profils à mixer',
            saveProfile: 'Enregistrer le profil',
            function: 'Fonction',
            functionToApply: 'Fonction à appliquer',
            graphProfileMouseActions:
                'Actions de la souris pour le profil graphique :\n- Ctrl+Défilement pour zoomer.\n- Clic gauche sur la ligne pour ajouter un point.\n- Clic droit sur un point pour le supprimer.\n- Glisser-déposer pour déplacer un point.',
            unsavedChanges: 'Des modifications non enregistrées ont été apportées à ce profil.',
            unsavedChangesHeader: 'Modifications non enregistrées',
            newProfile: 'Nouveau profil',
            tooltip: {
                profileType:
                    "Types de profils:<br/>- Par défaut: Non géré, rend le contrôle au pilote du périphérique<br/>- Fixe: définit une vitesse constante<br/>- Graphique: courbe de ventilateur personnalisable<br/>- Mélange: combine plusieurs profils<br/>- Superposition: applique un décalage à la sortie d'un profil existant",
            },
            profileDeleted: 'Profil supprimé',
            profileDuplicated: 'Profil dupliqué',
            usedBy: 'Utilisé par',
            deleteProfileConfirm: 'Êtes-vous sûr de vouloir supprimer : "{name}" ?',
            deleteProfileWithChannelsConfirm:
                '"{name}" est actuellement utilisé par : {channels}.\nLa suppression de ce profil réinitialisera les paramètres de ces canaux.\nÊtes-vous sûr de vouloir supprimer "{name}" ?',
            profileUpdated: 'Profil mis à jour avec succès',
            profileUpdateError:
                'Une erreur est survenue lors de la tentative de mise à jour de ce profil',
            tempSourceRequired: 'Une source de température est requise pour un profil graphique.',
            memberProfilesRequired: 'Au moins 2 profils membres sont requis pour un profil mixte.',
            minProfileTemp: 'Température de Profil Minimum',
            maxProfileTemp: 'Température de Profil Maximum',
            staticOffset: 'Décalage statique',
            offsetType: 'Type de décalage',
            offsetTypeStatic: 'Décalage statique',
            offsetTypeGraph: 'Décalage du graphique',
            baseProfile: 'Profil de base',
            baseProfileRequired: 'Un profil de base est requis pour un profil de superposition.',
            profileOutputDuty: 'Puissance de sortie du profil',
            offsetDuty: 'Puissance de décalage',
            points: 'Points',
            moveTable: 'Déplacer vers un autre coin',
            addPointAfter: 'Ajouter un point après',
            removePoint: 'Supprimer le point',
            curvePointLimitBadge: 'max {n} pts',
            curveLimitedByAmdGpu:
                'Courbe limitée à {n} points par la courbe de ventilateur matérielle du GPU AMD.',
            curveLimitedByFirmware:
                "Courbe limitée à {n} points par la courbe de ventilateur du firmware de l'appareil.",
        },
        modes: {
            createMode: 'Créer un Mode',
            editMode: 'Modifier le Mode',
            updateToCurrent: 'Enregistrer les réglages actuels dans le mode',
            deleteMode: 'Supprimer le Mode',
            deleteModeConfirm: 'Êtes-vous sûr de vouloir supprimer le Mode : "{name}" ?',
            updateModeConfirm:
                'Êtes-vous sûr de vouloir écraser "{name}" avec la configuration actuelle ?',
            duplicateMode: 'Dupliquer le Mode',
        },
        functions: {
            createFunction: 'Créer une Fonction',
            deleteFunction: 'Supprimer la Fonction',
            saveFunction: 'Enregistrer la Fonction',
            stepSizeTitle: 'Taille du Pas',
            fixedStepSize: 'Fixe',
            fixedStepSizeTooltip:
                'Activé utilise une taille de pas fixe pour tous les changements.\nDésactivé permet de définir une plage de taille de pas minimale et maximale.',
            asymmetric: 'Asymétrique',
            asymmetricTooltip:
                "Lorsqu'activé, des limites de taille de pas séparées peuvent être configurées pour les augmentations et diminutions de vitesse.\nUtile lorsque vous souhaitez que les ventilateurs accélèrent rapidement mais ralentissent progressivement, ou vice versa.",
            stepSizeMin: 'Minimum',
            stepSizeMinTooltip:
                'Le plus petit changement de vitesse du ventilateur qui sera appliqué.\nLes changements plus petits sont ignorés pour réduire les ajustements inutiles.',
            stepSizeMax: 'Maximum',
            stepSizeMaxTooltip:
                'Le plus grand changement de vitesse du ventilateur autorisé par mise à jour.\nLes changements plus importants sont limités à cette valeur pour des transitions plus douces.',
            stepSizeFixed: 'Taille',
            stepSizeFixedTooltip:
                'Une taille de pas unique appliquée à tous les changements de vitesse du ventilateur.\nTous les ajustements seront limités exactement à cette valeur.',
            stepSizeFixedIncreasing: 'Croissant',
            stepSizeFixedIncreasingTooltip:
                'Taille de pas fixe lorsque la vitesse du ventilateur augmente.\nTous les ajustements à la hausse seront limités exactement à cette valeur.',
            stepSizeFixedDecreasing: 'Décroissant',
            stepSizeFixedDecreasingTooltip:
                'Taille de pas fixe lorsque la vitesse du ventilateur diminue.\nTous les ajustements à la baisse seront limités exactement à cette valeur.',
            stepSizeMinIncreasing: 'Minimum Croissant',
            stepSizeMinIncreasingTooltip:
                'Taille de pas minimale lorsque la vitesse du ventilateur augmente.\nLes changements calculés plus petits sont ignorés pour réduire les ajustements inutiles.',
            stepSizeMaxIncreasing: 'Maximum Croissant',
            stepSizeMaxIncreasingTooltip:
                'Taille de pas maximale lorsque la vitesse du ventilateur augmente.\nLimite la rapidité avec laquelle les ventilateurs peuvent accélérer par mise à jour.',
            stepSizeMinDecreasing: 'Minimum Décroissant',
            stepSizeMinDecreasingTooltip:
                'Taille de pas minimale lorsque la vitesse du ventilateur diminue.\nLes changements calculés plus petits sont ignorés pour réduire les ajustements inutiles.',
            stepSizeMaxDecreasing: 'Maximum Décroissant',
            stepSizeMaxDecreasingTooltip:
                'Taille de pas maximale lorsque la vitesse du ventilateur diminue.\nLimite la rapidité avec laquelle les ventilateurs peuvent ralentir par mise à jour.',
            hysteresis: 'Hystérésis Avancée',
            hysteresisThreshold: 'Seuil',
            hysteresisThresholdTooltip:
                "Changement de température minimum (°C) requis avant d'ajuster la vitesse du ventilateur.\nAide à prévenir les fluctuations rapides de vitesse du ventilateur dues aux petites variations de température.",
            hysteresisDelay: 'Délai',
            hysteresisDelayTooltip:
                "Délai de réponse (secondes) avant d'appliquer les changements de vitesse du ventilateur.\nLes pics de température temporaires dans ce délai sont ignorés, lissant les fluctuations.",
            onlyDownward: 'Seulement Descendant',
            onlyDownwardTooltip:
                "Appliquer les paramètres d'hystérésis uniquement lorsque la température diminue.",
            stepOverrides: 'Dérogations de pas',
            thresholdHopping: 'Saut de Seuil',
            thresholdHoppingTooltip:
                "Lorsque la vitesse du ventilateur reste inchangée pendant 30+ secondes, les limites de taille de pas minimale et d'hystérésis sont temporairement contournées.\nCela garantit que les ventilateurs atteignent finalement leur vitesse cible, même avec des paramètres de seuil conservateurs. La taille de pas maximale est toujours respectée.",
            bypassMinAtExtremes: 'Toujours appliquer 0% / 100%',
            bypassMinAtExtremesTooltip:
                "Lorsque activé, les cycles cibles de 0% ou 100% sont appliqués même lorsque le changement est inférieur à la taille de pas minimale.\nUtile pour s'assurer que les ventilateurs s'arrêtent complètement ou atteignent leur RPM maximum. Désactivé par défaut.",
            unsavedChanges: 'Il y a des changements non enregistrés apportés à cette Fonction.',
            unsavedChangesHeader: 'Changements non enregistrés',
            functionError: 'Erreur lors de la tentative de mise à jour de cette fonction',
            newFunction: 'Nouvelle Fonction',
            functionDeleted: 'Fonction Supprimée',
            functionDuplicated: 'Fonction Dupliquée',
            usedBy: 'Utilisée par',
            deleteFunctionConfirm: 'Êtes-vous sûr de vouloir supprimer "{name}" ?',
            deleteFunctionWithProfilesConfirm:
                '"{name}" est actuellement utilisée par les Profils : {profiles}.\nLa suppression de cette Fonction réinitialisera les Fonctions de ces Profils.\nÊtes-vous sûr de vouloir supprimer "{name}" ?',
        },
        error: {
            accessDenied: 'Accès Refusé',
            accessDeniedMessage:
                "L'authentification a échoué. Veuillez vérifier votre mot de passe et réessayer.",
            connectionError: 'Erreur de Connexion CoolerControl',
            pageNotFound: 'Page Non Trouvée',
            returnToDashboard: 'Retour au Tableau de Bord',
            connectionErrorMessage: 'Impossible de se connecter au Daemon CoolerControl.',
            serviceRunningMessage:
                "Veuillez vérifier si le service daemon est en cours d'exécution.",
            checkProjectPage: "Pour obtenir de l'aide pour configurer le daemon, consultez la",
            projectPage: 'page du projet',
            helpfulCommands: 'Commandes utiles :',
            nonStandardAddress:
                'Si vous avez une adresse de daemon non standard, vous pouvez la spécifier ci-dessous :',
            daemonAddressDesktop: 'Adresse du Daemon (Application de Bureau)',
            daemonAddressWeb: 'Adresse du Daemon (Interface Web)',
            addressTooltip: "L'adresse IP ou le nom de domaine pour établir une connexion.",
            portTooltip: 'Le port pour établir une connexion.',
            sslTooltip: 'Se connecter au daemon en utilisant SSL/TLS.',
            saveTooltip: "Enregistrer les paramètres et recharger l'interface utilisateur",
            resetTooltip: 'Réinitialiser aux paramètres par défaut',
        },
        mode: {
            activateMode: 'Activer le mode',
            currentlyActive: 'Actuellement actif',
            modeHint:
                "Remarque : Les modes n'incluent pas les paramètres de Profil ou de Fonction, seulement les configurations de canal.",
        },
        lighting: {
            saveLightingSettings: "Enregistrer les paramètres d'éclairage",
            lightingMode: "Mode d'éclairage",
            speed: 'Vitesse',
            direction: 'Direction',
            forward: 'Avant',
            backward: 'Arrière',
            numberOfColors: 'Nombre de couleurs',
            numberOfColorsTooltip: "Nombre de couleurs à utiliser pour le mode d'éclairage choisi.",
        },
        lcd: {
            saveLcdSettings: 'Enregistrer les Paramètres LCD',
            lcdMode: 'Mode LCD',
            brightness: 'Luminosité',
            brightnessPercent: 'Pourcentage de Luminosité',
            orientation: 'Orientation',
            orientationDegrees: 'Orientation en degrés',
            chooseImage: 'Choisir une Image',
            dragAndDrop: 'Glissez et déposez les fichiers ici.',
            tempSource: 'Source de Température',
            tempSourceTooltip: "Source de température à utiliser dans l'affichage LCD.",
            imagesPath: 'Chemin des Images',
            imagesPathTooltip:
                'Entrez le chemin absolu vers le répertoire contenant les images.\nLe répertoire doit contenir au moins un fichier image, et ils\npeuvent être des images statiques ou des gifs. Le Carrousel les parcourra\navec le délai sélectionné. Tous les fichiers sont traités\nlors de la soumission pour assurer une compatibilité maximale.',
            browse: 'Parcourir',
            browseTooltip: "Parcourir pour un répertoire d'images",
            delayInterval: 'Intervalle de Délai',
            delayIntervalTooltip:
                "Nombre minimum de secondes de délai entre les changements d'image.\nNotez que le délai réel peut être plus long en raison du taux de sondage du daemon.",
            processing: 'Traitement en cours...',
            applying: 'Application en cours...',
            unsavedChanges: 'Il y a des changements non enregistrés apportés à ces Paramètres LCD.',
            unsavedChangesHeader: 'Changements non enregistrés',
            imageTooLarge: "L'image est trop grande. Veuillez en choisir une plus petite.",
            notImageType: "L'image n'est pas reconnue comme un type d'image",
            gifNotSupported:
                'Le micrologiciel de cet écran ne peut pas afficher de gifs. Choisissez une image fixe.',
        },
        shortcuts: {
            browserHint:
                "Dans un navigateur web, utilisez plutôt Ctrl+Alt+chiffre (les navigateurs réservent Ctrl+chiffre pour le changement d'onglet).",
            shortcuts: 'Raccourcis clavier',
            ctrl: 'Ctrl',
            comma: ',',
            viewShortcuts: 'Raccourcis clavier',
            settings: 'Paramètres',
        },
    },
    components: {
        aseTek690: {
            sameDeviceID:
                "Les anciens NZXT Kraken et l'EVGA CLC ont le même ID de périphérique et CoolerControl ne peut pas déterminer quel appareil est connecté. Cela est nécessaire pour une bonne communication avec l'appareil.",
            restartRequired:
                'Un redémarrage des services systemd de CoolerControl peut être nécessaire et sera géré automatiquement si besoin.',
            deviceModel: "Le périphérique Liquidctl est-il l'un des modèles suivants ?",
            modelList: 'NZXT Kraken X40, X60, X31, X41, X51 ou X61',
            acceptLabel: "Oui, c'est un appareil Kraken ancien",
            rejectLabel: "Non, c'est un appareil EVGA CLC",
        },
        password: {
            forgotPassword: 'Mot de passe oublié ?',
            forgotPasswordHelpIntro:
                "Exécutez cette commande dans un terminal en tant que root, puis cliquez sur Recharger l'UI :",
            forgotPasswordCopyCommand: 'Copier la commande',
            forgotPasswordCommandCopied: 'Commande copiée dans le presse-papiers',
            forgotPasswordReloadButton: "Recharger l'UI",
            continueButton: 'Continuer',
            backButton: 'Retour',
            passwordMismatch: 'Les mots de passe ne correspondent pas',
        },
        notFound: {
            message: "Tout comme la distribution Linux 🐧 parfaite,\ncette page n'existe pas.",
        },
        deviceInfo: {
            details: "Détails de l'Appareil",
            systemName: 'Nom du Système',
            deviceType: "Type d'Appareil",
            deviceUID: "UID de l'Appareil",
            firmwareVersion: 'Version du Firmware',
            model: 'Modèle',
            driverName: 'Nom du Pilote',
            driverType: 'Type de Pilote',
            driverVersion: 'Version du Pilote',
            locations: 'Emplacements',
        },
        onboarding: {
            search: 'Recherche',
            searchDesc:
                "Trouvez ici n'importe quel appareil, capteur, paramètre ou action. Appuyez sur Ctrl+K depuis n'importe où dans l'application.",
            welcome: 'Bienvenue dans CoolerControl !',
            gettingStartedIntro:
                "Faites une visite rapide pour vous orienter. Elle parcourt la barre de navigation et les principales zones de l'application.",
            startTourAgain:
                'Vous pouvez relancer cette visite à tout moment depuis les Paramètres.',
            startTour: 'Démarrer la visite',
            maybeLater: 'Peut-être plus tard',
            openGettingStarted: 'Ouvrir la Documentation',
            finishLater: 'Compris, merci',
            home: 'Accueil',
            homeDesc:
                "La page d'accueil de l'application : état du daemon et santé des périphériques en un coup d'œil, plus les journaux, les infos de l'app, des liens utiles et des outils de test de charge.",
            cooling: 'Refroidissement',
            coolingDesc:
                "Votre centre de contrôle des ventilateurs : ajustez les vitesses des ventilateurs et des pompes, et appliquez des Profils et des Fonctions à n'importe quel canal.",
            monitoring: 'Surveillance',
            monitoringDesc:
                'Créez des Tableaux de bord, surveillez chaque capteur et configurez des Alertes pour suivre votre système en temps réel.',
            devices: 'Appareils',
            devicesDesc:
                "Passez en revue le matériel détecté, configurez les fonctionnalités propres à chaque appareil comme l'éclairage RGB et les écrans LCD, et créez des Capteurs Personnalisés.",
            plugins: 'Plugins',
            pluginsDesc: 'Parcourez et ouvrez les plugins installés qui étendent CoolerControl.',
            settings: 'Paramètres',
            settingsDesc:
                "Configurez les préférences de l'interface, les options du daemon et le comportement du système.",
            access: 'Accès',
            accessDesc:
                "Connectez-vous ou déconnectez-vous, changez votre mot de passe et gérez les Jetons d'accès qui donnent aux outils et plugins l'accès à l'API.",
            restartMenu: 'Menu de Redémarrage',
            restartMenuDesc: "Rechargez l'interface ou redémarrez le daemon système si nécessaire.",
            modes: 'Modes',
            modesDesc:
                'Les Modes sont des collections enregistrées de vos paramètres. Basculez ici entre des configurations comme Silencieux et Performance, ou gérez-les.',
            thatsIt: "C'est tout !",
            startNow:
                'Vous êtes prêt. Ouvrez la documentation pour en savoir plus, ou lancez-vous et configurez vos appareils.',
        },
        axisOptions: {
            title: "Options d'Axe",
            autoScale: 'AutoÉchelle',
            max: 'Max',
            min: 'Min',
            dutyTemperature: 'Cycle / Température',
            rpmMhz: 'tr/min / MHz',
            krpmGhz: 'k tr/min / GHz',
            watts: 'watts',
        },
        sensorTable: {
            device: 'Appareil',
            channel: 'Canal',
            current: 'Actuel',
            range: 'Plage',
            average: 'Moyenne',
            resetStats: 'Réinitialiser',
            resetStatsTooltip: 'Réinitialiser min/max/moyenne pour tous les canaux',
        },
        modeTable: {
            setting: 'Paramètre',
        },
        menuTagAssign: {
            title: 'Attribuer des Tags',
            noTags: 'Aucun tag pour le moment.',
            tagName: 'Nom du tag',
            editTag: 'Modifier le tag',
            deleteTag: 'Supprimer le tag',
        },
        wizards: {
            calibration: {
                title: 'Étalonner les ventilateurs',
                pickIntro:
                    'Sélectionnez les ventilateurs à étalonner. Les ventilateurs déjà étalonnés et ceux contrôlés par le firmware sont décochés par défaut.',
                noFans: 'Aucun ventilateur contrôlable détecté.',
                selectAll: 'Tout sélectionner',
                calibratedBadge: 'étalonné',
                firmwareControlledBadge: 'contrôlé par le firmware',
                firmwareControlledDesc:
                    "Le firmware gère le profil de ce canal. Un étalonnage s'applique toujours : sa conversion de rapport cyclique est intégrée à la courbe transmise au firmware. L'impulsion de démarrage, non, car une courbe de firmware ne peut pas l'exprimer.",
                blockedByAlert: "bloqué : l'alerte '{name}' est active",
                alertsPausedNote:
                    '{count} alerte(s) surveillent les ventilateurs sélectionnés et sont mises en pause pendant le balayage de chaque ventilateur.',
                idleNote:
                    "L'étalonnage fait varier chaque ventilateur sur toute sa plage. À exécuter de préférence au repos : c'est bruyant et prend quelques minutes par ventilateur.",
                concurrencyLabel: 'Ventilateurs à la fois',
                concurrencyNote:
                    'Plusieurs à la fois est plus rapide, mais des ventilateurs adjacents peuvent fausser les mesures les uns des autres (vent croisé, push-pull). Un à la fois est le plus précis.',
                start: 'Démarrer',
                close: 'Fermer',
                running: 'Étalonnage de {current} sur {total}...',
                queued: "En file d'attente",
                done: 'Terminé',
                failed: 'Échec',
                skipped: 'Ignoré',
                startFailed: 'Impossible de démarrer',
                summary: '{done} étalonnés, {failed} en échec, {skipped} ignorés.',
                reloadBatch:
                    '{count} ventilateurs étalonnés. Recharger pour appliquer le nouveau contrôle normalisé par RPM ?',
                stagePreflight: 'Pré-vérification',
                stageUpSweep: 'Balayage ascendant',
                stageDownSweep: 'Balayage descendant',
                stageFinalizing: 'Finalisation',
            },
            fanControl: {
                fanControlWizard: 'Assistant de Contrôle des Ventilateurs',
                editCurrentProfile: 'Modifier le Profil',
                editCurrentFunction: 'Modifier la Fonction',
                currentSettings: 'Paramètres Actuels',
                manualSpeed: 'Vitesse Manuelle',
                createNewProfile: 'Nouveau Profil',
                existingProfile: 'Choisir un Profil',
                resetSettings: 'Réinitialiser à Non géré',
                chooseProfileNameType: 'Choisir un Nom et un Type de Profil',
                newDefaultProfile: 'Nouveau Profil par Défaut',
                profileCreatedApplied: 'Profil créé et appliqué',
                willCreatedAndAppliedTo: 'sera créé et appliqué à',
                newFixedProfile: 'Nouveau profil fixe',
                withSettings: 'avec les paramètres suivants',
                selectSpeed: 'Sélectionnez votre vitesse',
                newMixProfile: 'Nouveau profil de mélange',
                newGraphProfile: 'Nouveau profil graphique',
                newOverlayProfile: 'Nouveau profil de superposition',
                functionFor: 'Choisissez une fonction à appliquer à',
                functionDescription:
                    'Les fonctions ajustent la manière dont votre Profil est appliqué, comme le temps de réponse et le cycle minimum.',
                createNewFunction: 'Nouvelle Fonction',
                existingFunction: 'Choisir une Fonction',
                defaultFunction: 'Fonction par Défaut',
                chooseFunctionName: 'Choisissez un nom de fonction',
                newFunctionName: 'Fonction pour {profileName}',
                summary: 'Résumé',
                aNewProfile: 'Un nouveau profil',
                andFunction: 'et fonction',
            },
            profile: {
                willCreated: 'sera créé.',
            },
            profileApply: {
                applyProfile: 'Appliquer le Profil',
                channelsApply: 'Canaux pour Appliquer le Profil',
                selectChannels: 'Sélectionner les Canaux',
                channelsTooltip: 'Sélectionnez un ou plusieurs canaux pour appliquer ce Profil.',
                selectByTag: 'Sélectionner par tag',
                selectByChannel: 'Sélectionner par canal',
                tagFanCount: '{count} canal | {count} canaux',
                noTags: 'Aucun tag configuré.',
            },
            functionApply: {
                applyFunction: 'Appliquer la Fonction',
                profilesApply: 'Profils pour Appliquer la Fonction',
                selectProfiles: 'Sélectionner les Profils',
                profilesTooltip:
                    'Sélectionnez un ou plusieurs Profils pour appliquer cette Fonction.',
            },
            generate: {
                title: 'Créer des profils automatiquement',
                assignIntro:
                    "Attribuez un rôle à chaque ventilateur. Laissez un ventilateur sans rôle pour l'ignorer.",
                calibrateFirst:
                    "Étalonnez d'abord les ventilateurs pour une meilleure cohérence (quelques minutes)",
                skip: 'Ignorer',
                noFans: 'Aucun ventilateur contrôlable détecté.',
                tempsIntro:
                    "Choisissez les températures que votre configuration doit suivre. Laissez-en une vide pour l'exclure : un système à graphiques intégrés n'a pas besoin de temp. GPU, et c'est le fait d'en choisir une qui implique le GPU dans les courbes du radiateur AIO et des ventilateurs du boîtier.",
                cpuTemp: 'Temp. CPU',
                gpuTemp: 'Temp. GPU',
                liquidTemp: 'Temp. du liquide',
                ambientTemp: 'Temp. ambiante (facultatif)',
                tempNone: 'Aucune',
                presetIntro: "Choisissez l'agressivité de la montée en régime des ventilateurs.",
                perKindOverrides: 'Remplacements par rôle (avancé)',
                cfmCaveat:
                    "Le biais de pression positive est basé sur le rapport cyclique (duty), pas sur le flux d'air : avec des nombres de ventilateurs déséquilibrés, il ne peut pas garantir une pression positive.",
                previewIntro:
                    "Vérifiez ce qui sera créé et appliqué. Rien n'est enregistré tant que vous n'avez pas confirmé.",
                previewAssignments: 'Attributions des ventilateurs',
                reusedHeader: 'Existe déjà',
                reused: 'réutilisé',
                willCreateHeader: 'Sera créé',
                startingPointNote:
                    'Un point de départ général pour votre configuration de ventilateurs, destiné à être ajusté plutôt que laissé tel quel.',
                replaces: 'remplace {name}',
                generated: '{count} profils générés.',
                generateError: 'Impossible de générer les profils.',
                applyError: 'Impossible de créer les profils.',
                kind: {
                    CpuCooler: 'Refroidisseur à air CPU',
                    GpuFan: 'Ventilateur GPU',
                    AioRadiator: 'Radiateur AIO',
                    AioPump: 'Pompe AIO',
                    CaseIntake: 'Admission du boîtier',
                    CaseExhaust: 'Extraction du boîtier',
                    LaptopFan: 'Ventilateur de portable',
                },
            },
        },
        channelExtensionSettings: {
            title: "Paramètres du canal de l'appareil",
            firmwareControlledProfile: 'Profil contrôlé par le firmware',
            firmwareControlledProfileDesc:
                "Lorsque cette option est activée, le firmware de l'appareil gère le profil du ventilateur.\nUtile pour le matériel qui réagit mal aux modifications fréquentes de vitesse effectuées par le logiciel.\nDisponible uniquement pour les profils Graph qui utilisent des capteurs de température internes à l'appareil.\nLes paramètres de Fonction ne s'appliquent pas.\nSur un canal étalonné, les points de la courbe sont convertis par l'étalonnage, mais l'impulsion de démarrage ne s'applique pas.",
            saveError: "Échec de l'enregistrement des paramètres de l'extension de canal",
            firmwareControlDisabled:
                "Le contrôle par firmware n'est pas disponible avec la configuration actuelle.\nUtilisez un profil Graph pour cet appareil avec un capteur de température interne pris en charge.",
            calibration: {
                heading: 'Étalonnage des RPM',
                description:
                    "Faites parcourir au ventilateur sa plage complète pour obtenir sa véritable courbe rapport cyclique/RPM, puis pilotez le canal en rapport cyclique réel normalisé par les RPM.\nSupprime les zones mortes à bas rapport cyclique et la saturation à haut rapport cyclique.\nLe coup d'envoi est aussi géré automatiquement quand le ventilateur est étalonné : une brève impulsion de démarrage le lance depuis l'arrêt avant qu'il ne se stabilise à la valeur cible.\nLe balayage prend généralement plusieurs minutes, et peut durer sensiblement plus longtemps pour des ventilateurs à réaction lente. Le canal est réglé à 0 % au début.",
                statusNotCalibrated: 'Non étalonné',
                blockedByAlert:
                    "L'étalonnage est bloqué : l'alerte '{name}' est active sur ce ventilateur.",
                alertsPausedNote:
                    'Les alertes surveillant ce ventilateur sont mises en pause pendant le balayage.',
                statusInProgress: 'Étalonnage en cours : {stage} ({percent} %)',
                statusCompleted: 'Étalonné (lisse, mappage actif)',
                statusCompletedStepped: 'Étalonné (courbe en marches, mappage désactivé)',
                statusCompletedWithWarnings: 'Étalonné avec avertissements : {messages}',
                statusFailed: 'Dernière tentative échouée : {message}',
                warningNoTachometer:
                    'aucune RPM détectée (le capteur ou le câblage peuvent être débranchés)',
                warningNotControllable:
                    'le ventilateur ne réagit pas au rapport cyclique (probablement piloté par le BIOS)',
                warningLimitedRange:
                    'plage de RPM limitée ({span} RPM) ; résolution de mappage grossière',
                warningOscillating:
                    'le ventilateur oscille entre {lower} % et {upper} % de rapport cyclique (impulsion contrôlée par le firmware) ; mappage désactivé à bas rapport cyclique',
                warningImplausibleCurve:
                    'la courbe mesurée est plate, inversée ou son minimum ne laisse aucune plage utilisable (les mesures de tr/min semblent peu fiables) ; mappage désactivé, recalibrez pour réessayer',
                stagePreflight: 'pré-vol',
                stageUpSweep: 'balayage montant',
                stageDownSweep: 'balayage descendant',
                stageFinalizing: 'finalisation',
                buttonCalibrate: 'Étalonner',
                buttonRecalibrate: 'Ré-étalonner',
                buttonCancel: 'Annuler',
                buttonClear: 'Effacer',
                clearConfirm:
                    "Effacer l'étalonnage de {channel} ? Le relancer prend plusieurs minutes.",
                buttonViewCurve: 'Voir la courbe',
                caveatsBanner:
                    "Étalonner plusieurs ventilateurs de refroidissement principaux en même temps peut faire monter la température du système.\nDes ventilateurs push-pull de radiateur diagnostiqués en parallèle peuvent produire des mesures inexactes.\nMaintenez le système au repos pendant l'étalonnage.",
                clearedNotice:
                    "Effacé. Les courbes de ventilateur de ce canal pilotent à nouveau directement le rapport cyclique de l'appareil.",
                startError: "Échec du démarrage de l'étalonnage",
                cancelError: "Échec de l'annulation de l'étalonnage",
                clearError: "Échec de l'effacement de l'étalonnage",
                reloadHeader: "Recharger l'interface",
                reloadAccept: 'Recharger',
                reloadReject: 'Plus tard',
                reload_rpm_only_completed_single:
                    "Étalonnage terminé pour {channelName}. Rechargez l'interface pour afficher le graphique de rapport cyclique du canal.",
                reload_rpm_only_completed_multi:
                    "Étalonnage terminé pour {channelList}. Rechargez l'interface pour afficher le graphique de rapport cyclique de chaque canal.",
                reload_rpm_only_cleared_single:
                    "Étalonnage effacé pour {channelName}. Rechargez l'interface pour supprimer le graphique de rapport cyclique désormais obsolète du canal.",
                reload_rpm_only_cleared_multi:
                    "Étalonnage effacé pour {channelList}. Rechargez l'interface pour supprimer le graphique de rapport cyclique désormais obsolète de chaque canal.",
                reload_duty_range_completed_single:
                    "Étalonnage terminé pour {channelName}. Rechargez l'interface pour que le curseur de rapport cyclique manuel et l'assistant de contrôle du ventilateur prennent en compte la nouvelle plage du canal.",
                reload_duty_range_completed_multi:
                    "Étalonnage terminé pour {channelList}. Rechargez l'interface pour que le curseur de rapport cyclique manuel et l'assistant de contrôle du ventilateur prennent en compte la nouvelle plage de chaque canal.",
                reload_duty_range_cleared_single:
                    "Étalonnage effacé pour {channelName}. Rechargez l'interface pour que le curseur de rapport cyclique manuel revienne aux limites matérielles du canal.",
                reload_duty_range_cleared_multi:
                    "Étalonnage effacé pour {channelList}. Rechargez l'interface pour que le curseur de rapport cyclique manuel revienne aux limites matérielles de chaque canal.",
                reload_mixed_multi:
                    "Étalonnage modifié pour {channelList}. Rechargez l'interface pour que chaque canal prenne en compte son nouvel affichage de rapport cyclique et les limites du curseur.",
            },
        },
        calibrationCurve: {
            dialogTitle: "Courbe d'étalonnage",
            loading: "Chargement de l'étalonnage...",
            notFound: "Aucune donnée d'étalonnage trouvée pour ce canal.",
            loadError: "Échec du chargement des données d'étalonnage.",
            axisDuty: 'Rapport cyclique',
            axisRpm: 'RPM',
            legendUp: 'Balayage montant',
            legendDown: 'Balayage descendant',
            markerStable: 'Seuil stable',
            curveKindSmooth: 'Lisse (mappage actif)',
            curveKindStepped: 'En marches (mappage désactivé)',
            fieldCurveKind: 'Courbe',
            fieldCurveKindTooltip:
                'Manière dont le canal réagit aux changements de rapport cyclique.\nLes ventilateurs lisses ont une courbe rapport-cyclique-à-RPM continue, le dispatcher mappe donc le rapport cyclique cible via la calibration. Les ventilateurs en marches ont des plateaux RPM discrets, les rapports cycliques sont donc transmis sans modification.',
            fieldRpmMax: 'RPM maximales',
            fieldRpmMaxTooltip:
                "RPM les plus élevées observées pendant le balayage.\nUtilisées comme référence 100% lors de la conversion d'un rapport cyclique cible en sa valeur réelle normalisée par RPM.",
            fieldKick: "Durée de l'impulsion",
            fieldKickTooltip:
                "Durée pendant laquelle le dispatcher maintient le rapport cyclique d'impulsion avant de redescendre au maintien lors d'un démarrage à froid.\nMesurée en écrivant le rapport cyclique d'impulsion le plus défavorable (avec boost) du dispatcher depuis l'arrêt, puis en attendant que les RPM se stabilisent dans une fenêtre stable.",
            fieldStart: 'Rapport cyclique min. de démarrage',
            fieldStartTooltip:
                "Rapport cyclique le plus bas qui démarre le ventilateur de manière fiable depuis l'arrêt.\nEn dessous, le ventilateur peut ne pas commencer à tourner, même s'il continuerait à tourner s'il était déjà en marche.",
            fieldSustain: 'Rapport cyclique min. de maintien',
            fieldSustainTooltip:
                'Rapport cyclique le plus bas auquel le ventilateur continue de tourner une fois lancé.\nLe dispatcher ne descendra pas le rapport cyclique en cours en dessous de cette valeur, sauf si le canal est envoyé à 0.',
            fieldStable: 'Rapport cyclique min. stable',
            fieldStableTooltip:
                "Rapport cyclique le plus bas auquel le ventilateur fonctionne sans oscillation.\nLes ventilateurs pilotés par le firmware relèvent les RPM au-dessus d'un seuil interne à bas rapport cyclique, ce qui produit un battement audible ; le dispatcher plafonne le maintien post-impulsion à cette valeur pour que le ventilateur reste au-dessus de cette bande.",
            fieldSaturate: 'Rapport cyclique proche du plateau',
            fieldSaturateTooltip:
                "Rapport cyclique à partir duquel les gains de RPM commencent à diminuer.\nLe ventilateur peut encore ajouter quelques RPM au-delà de ce rapport cyclique jusqu'à 100 %, c'est pourquoi l'étalonnage utilise la plage complète de 0 à 100 %.",
            fieldTimestamp: 'Étalonné',
            overridesHeading: 'Surcharges',
            fieldKickBoostOverride: "Boost d'impulsion",
            fieldKickBoostOverrideTooltip:
                "Force l'activation ou la désactivation du boost d'impulsion au démarrage à froid pour ce canal, ou laisse le daemon décider d'après l'heuristique de la courbe montante.\nLe boost relève brièvement le rapport cyclique d'impulsion au-dessus du maintien pour pousser le ventilateur au-delà de son seuil d'inertie.",
            kickBoostAuto: 'Auto',
            kickBoostOn: 'Forcer activé',
            kickBoostOff: 'Forcer désactivé',
            fieldKickDurationOverride: "Surcharge de la durée d'impulsion",
            fieldKickDurationOverrideTooltip:
                "Surcharge la durée d'impulsion calibrée. Laisser vide pour utiliser la valeur mesurée.\nAllonger lorsque le ventilateur a besoin de plus de temps au rapport cyclique d'impulsion pour se stabiliser avant que le maintien prenne le relais.",
            kickDurationDefault: 'défaut',
            kickDurationReset: 'Réinitialiser par défaut',
            kickBoostCurrentlyOn: 'actuellement activé',
            kickBoostCurrentlyOff: 'actuellement désactivé',
            fieldWalkAfterKick: 'Descente progressive après impulsion',
            fieldWalkAfterKickTooltip:
                "Après la fenêtre d'impulsion, abaisse le rapport cyclique vers le maintien par petits incréments. Protège les ventilateurs dont les contrôleurs coupent l'alimentation lors d'une chute brutale.\nDésactiver pour passer directement de l'impulsion au maintien. Sans risque sur la plupart des ventilateurs PWM modernes et supprime la rampe descendante visible après chaque démarrage à froid.",
            overridesSaveFailed: 'Échec de la sauvegarde des surcharges de calibration',
        },
        deviceExtensionSettings: {
            title: 'Paramètres Avancés du Périphérique',
            directAccess: 'Accès Direct',
            directAccessDesc:
                "Lorsqu'il est activé, le pilote liquidctl ignorera le pilote du noyau HWMon\net communiquera directement avec le périphérique.\nCela peut être utile pour les périphériques qui ont des conflits lors de l'utilisation des deux pilotes.",
            useHwmon: 'Utiliser le pilote HWMon',
            useHwmonDesc:
                'Bascule le pilote de ce périphérique de liquidctl vers le pilote du noyau HWMon.\nCela peut améliorer les performances et la stabilité, mais peut réduire les fonctionnalités disponibles.',
            disableDevice: 'Désactiver le périphérique liquidctl',
            disableInfo:
                'La désactivation du pilote liquidctl désactivera ce périphérique. Un nouveau périphérique basé sur HWMon apparaîtra en bas du menu des périphériques. Vous pouvez réactiver le périphérique liquidctl à tout moment depuis le menu des paramètres.',
            commandDelay: 'Délai de commande',
            commandDelayDesc:
                'Délai en millisecondes entre les commandes envoyées à ce périphérique.\nCela peut aider les périphériques qui ont des problèmes de communication\nlorsque plusieurs commandes sont envoyées en succession rapide.',
            overdrive: 'GPU Overdrive',
            overdriveDesc:
                "Les GPU AMD RDNA3/4 nécessitent l'activation de l'overdrive pour le contrôle des ventilateurs.\nCeci configure le paramètre noyau amdgpu.ppfeaturemask\net nécessite un redémarrage du système.",
            overdriveEnable: 'Activer',
            overdriveActive: 'Actif',
            overdriveSuccess: 'Overdrive configuré',
            thinkPadFanControl: 'Contrôle du ventilateur',
            thinkPadFanControlDesc:
                'Active le contrôle du ventilateur ThinkPad ACPI.\nLe contrôle du ventilateur est désactivé par défaut pour des raisons de sécurité.\nProcédez à vos propres risques.',
            thinkPadFullSpeed: 'Pleine vitesse',
            thinkPadFullSpeedDesc:
                'Active le mode pleine vitesse pour les ventilateurs ThinkPad.\nPermet aux ventilateurs de tourner au maximum absolu à 100%,\nmais fait fonctionner les ventilateurs hors spécification avec une usure accrue.',
        },
    },
    auth: {
        enterPassword: 'Entrez Votre Mot de Passe',
        setNewPassword: 'Entrez Un Nouveau Mot de Passe',
        changeDefaultPassword:
            'Veuillez définir un mot de passe pour empêcher tout accès non autorisé. Celui-ci est distinct de votre compte système.',
        accessTokens: "Jetons d'accès",
        tokenLabel: 'Libellé (ex. cctv)',
        tokenExpiry: "Date d'expiration (facultatif)",
        createToken: 'Créer un jeton',
        tokenCreated: 'Jeton créé',
        tokenCreatedDetail: 'Copiez ce jeton maintenant. Il ne sera plus affiché.',
        tokenCopied: 'Jeton copié dans le presse-papiers',
        tokenDeleted: 'Jeton supprimé',
        tokenCreateError: 'Échec de la création du jeton',
        tokenDeleteError: 'Échec de la suppression du jeton',
        tokenLoadError: 'Échec du chargement des jetons',
        tokenDeleteConfirm:
            "Êtes-vous sûr de vouloir supprimer ce jeton ? Les services qui l'utilisent perdront l'accès.",
        tokenDeleteHeader: 'Supprimer le jeton',
        noTokens: "Aucun jeton d'accès créé pour le moment.",
        expires: 'Expire',
        expired: 'Expiré',
        active: 'Actif',
        never: 'Jamais',
        lastUsed: 'Dernière utilisation',
        neverUsed: 'Jamais utilisé',
        created: 'Créé',
        label: 'Libellé',
        actions: 'Actions',
        writeAccess: 'Accès en écriture',
        writeAccessTooltip:
            "Lorsqu'activé, ce jeton peut effectuer des modifications. Lorsque désactivé, le jeton peut uniquement lire les données.",
    },
    daemon: {
        status: {
            ok: 'Ok',
            hasWarnings: 'A des Avertissements',
            hasErrors: 'A des Erreurs',
        },
    },
    // Rendered by the Qt desktop app, which has no translation pipeline of its own.
    // Pushed over IPC and cached there. See shell/qtStrings.ts.
    desktop: {
        closePrompt: {
            title: 'Réduire dans la zone de notification ?',
            body: "Le démon CoolerControl continue de fonctionner en arrière-plan dans les deux cas, vos réglages de refroidissement restent donc actifs. Gardez l'interface dans la zone de notification pour un accès rapide et les notifications du bureau, ou quittez-la complètement.",
            keepInTray: 'Garder dans la zone de notification',
            quit: 'Quitter',
            remember: 'Mémoriser mon choix',
        },
        tray: {
            show: '&Afficher',
            hide: '&Masquer',
            daemonConnection: 'Connexion au &démon…',
            quit: '&Quitter',
            modes: 'Modes',
            sensors: 'Capteurs',
            daemons: 'Démons',
        },
        cert: {
            title: 'Certificat du démon non vérifié',
            changedTitle: 'Certificat modifié',
            // %1 is the daemon host, substituted by Qt via QString::arg.
            body: '%1 utilise un certificat auto-signé, qui ne peut pas être vérifié automatiquement. Ne continuez que si vous reconnaissez ce démon.',
            changedBody:
                "Le certificat de %1 n'est pas celui approuvé précédemment. Cela peut signifier que le démon a été réinstallé, ou que la connexion est interceptée.",
            fingerprint: 'Empreinte (SHA-256) :',
            trust: 'Faire confiance à ce certificat',
            cancel: 'Annuler',
        },
        wizard: {
            windowTitle: 'Erreur de connexion au démon',
            windowTitleOk: 'Connexion au démon',
            apply: '&Appliquer',
            retry: '&Réessayer',
            quitApp: '&Quitter',
            introPurpose:
                "Ces paramètres déterminent comment l'application de bureau se connecte au démon CoolerControl.",
            introFailed: "La connexion au démon CoolerControl n'a pas pu être établie.",
            introCheckService:
                'Veuillez vérifier que le service systemd est démarré et disponible.',
            // %1 is substituted by Qt via QString::arg, not by vue-i18n.
            introDocs: "Consultez le %1 pour les instructions d'installation.",
            introDocsLink: 'site de documentation',
            introCommands: "Quelques commandes utiles pour activer et vérifier l'état du démon :",
            introCustomAddress:
                'Si vous avez configuré une adresse non standard pour vous connecter au démon, vous pouvez la définir aux étapes suivantes :',
            lastError: 'Dernière erreur :',
            // %1 is substituted by Qt via QString::arg, not by vue-i18n.
            errorNotDaemon:
                "L'adresse a répondu, mais pas en tant que démon CoolerControl (HTTP %1).",
            errorCertUntrusted: "Le certificat du démon n'a pas été approuvé.",
            errorCertInvalid:
                "Le certificat du démon n'est pas valide et la validation des certificats est activée.",
            savedLabel: 'Connexion enregistrée :',
            newConnection: 'Nouvelle connexion…',
            removeConnection: 'Supprimer',
            removeConnectionTooltip: 'Oublier le démon sélectionné.',
            removeConnectionBody: 'Ne plus proposer ce démon dans la zone de notification ?',
            nameLabel: 'Nom :',
            nameTooltip: 'Libellé facultatif pour ce démon. Vide affiche hôte:port.',
            addressTitle: 'Adresse du démon - Application de bureau',
            addressSubtitle: "Ajustez les champs d'adresse si nécessaire.",
            hostLabel: "Adresse de l'hôte :",
            hostTooltip:
                "L'adresse IPv4, IPv6 ou le nom d'hôte à utiliser pour communiquer avec le démon.",
            portLabel: 'Port :',
            portTooltip: 'Le numéro de port à utiliser pour communiquer avec le démon.',
            sslTooltip: 'Activer ou désactiver SSL/TLS (HTTPS)',
            strictTls: 'Valider le certificat',
            strictTlsTooltip:
                'Exiger un certificat vérifiable normalement. Laissez désactivé pour utiliser le certificat auto-signé du démon, approuvé à la première connexion pour les démons distants.',
            defaults: 'Valeurs par défaut',
            defaultsTooltip: "Réinitialiser l'adresse du démon aux valeurs par défaut",
            forgetCerts: 'Oublier les certificats approuvés',
            forgetCertsTooltip:
                'Supprime les certificats des démons distants auxquels cette application fait confiance.',
            forgetCertsBody:
                "Ces certificats de démon sont actuellement approuvés. Les oublier signifie qu'une confirmation vous sera demandée à la prochaine connexion.",
        },
        versionMismatch: {
            title: 'Version incompatible',
            text: "La version de l'application de bureau (%1) ne correspond pas à la version du démon (%2).",
            informative:
                "Veuillez redémarrer l'application de bureau pour charger la bonne version de l'interface.",
            quitApp: '&Quitter',
            continueAnyway: 'Continuer quand même',
        },
    },
    device_store: {
        unauthorized: {
            summary: 'Session expirée',
            detail: 'Votre session a expiré. Rechargement pour se reconnecter.',
        },
        login: {
            failed: {
                summary: 'Échec de Connexion',
                detail: 'Mot de Passe Invalide',
            },
            rate_limited: {
                summary: 'Connexion Temporairement Bloquée',
            },
        },
        logout: {
            summary: 'Déconnexion',
            detail: 'Vous vous êtes déconnecté avec succès.',
        },
        password: {
            set_success: {
                summary: 'Mot de Passe',
                detail: 'Nouveau mot de passe défini avec succès',
            },
        },
        asetek: {
            header: 'Appareil Inconnu Détecté',
            success: {
                summary: 'Succès',
                detail_legacy:
                    "Type de modèle d'appareil défini avec succès. Redémarrage en cours.",
                detail_evga: "Type de modèle d'appareil défini avec succès.",
            },
            error: {
                summary: 'Erreur',
                detail: 'Processus interrompu.',
            },
        },
    },
    models: {
        chartType: {
            timeChart: 'Graphique Temporel',
            table: 'Tableau',
        },
        dataType: {
            temp: 'Temp',
            duty: 'Cycle',
            load: 'Charge',
            rpm: 'tr/min',
            freq: 'Fréq',
            watts: 'Watts',
        },
        profile: {
            profileType: {
                default: 'Par Défaut',
                fixed: 'Fixe',
                graph: 'Graphique',
                mix: 'Mélange',
                overlay: 'Superposition',
            },
            mixFunctionType: {
                min: 'Minimum',
                max: 'Maximum',
                avg: 'Moyenne',
                diff: 'Différence',
                sum: 'Somme',
            },
        },
        customSensor: {
            sensorType: {
                mix: 'Mélange',
                file: 'Fichier',
                offset: 'Décalage',
                timeAverage: 'Moyenne Temporelle',
                exponentialMovingAvg: 'Moyenne Mobile Exponentielle',
            },
            mixFunctionType: {
                min: 'Minimum',
                max: 'Maximum',
                delta: 'Delta',
                avg: 'Moyenne',
                weightedAvg: 'Moyenne Pondérée',
            },
        },
        themeMode: {
            system: 'Système',
            dark: 'Sombre',
            light: 'Clair',
            highContrastDark: 'Sombre à Haut Contraste',
            highContrastLight: 'Clair à Haut Contraste',
            custom: 'Thème Personnalisé',
        },
        interfaceFont: {
            bundled: 'Fournie (IBM Plex)',
            system: 'Système',
        },
        channelViewType: {
            control: 'Contrôle',
            dashboard: 'Tableau de Bord',
        },
        startupPage: {
            appInfo: 'Info & Outils',
            homeDashboard: "Tableau de bord d'accueil",
            controls: 'Contrôles',
        },
        alertState: {
            active: 'Actif',
            inactive: 'Inactif',
            error: 'Erreur',
        },
        pluginStatus: {
            running: 'En cours',
            stopped: 'Arrêté',
            unmanaged: 'Non géré',
            disabled: 'Désactivé',
        },
        deviceType: {
            customSensors: 'Capteurs Personnalisés',
            cpu: 'CPU',
            gpu: 'GPU',
            liquidctl: 'Liquidctl',
            hwmon: 'Hwmon',
            servicePlugin: 'Plugin de Service',
        },
        driverType: {
            kernel: 'Noyau',
            liquidctl: 'Liquidctl',
            nvml: 'NVML',
            nvidiaCli: 'Nvidia CLI',
            coolercontrol: 'CoolerControl',
            external: 'Externe',
        },
        lcdModeType: {
            none: 'Aucun',
            liquidctl: 'Liquidctl',
            custom: 'Personnalisé',
        },
        channelType: {
            lcd: 'LCD',
        },
    },
}

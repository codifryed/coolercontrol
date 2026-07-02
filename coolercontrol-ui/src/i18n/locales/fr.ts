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
        save: 'Enregistrer',
        cancel: 'Annuler',
        confirm: 'Confirmer',
        delete: 'Supprimer',
        edit: 'Modifier',
        add: 'Ajouter',
        remove: 'Retirer',
        yes: 'Oui',
        no: 'Non',
        ok: 'OK',
        error: 'Erreur',
        success: 'Succès',
        warning: 'Avertissement',
        loading: 'Chargement...',
        restarting: 'Redémarrage...',
        noData: 'Aucune donnée disponible',
        retry: 'Réessayer',
        saveAndRefresh: 'Enregistrer et actualiser',
        reset: 'Réinitialiser',
        back: 'Retour',
        sslTls: 'SSL/TLS',
        protocol: 'Protocole',
        address: 'Adresse',
        port: 'Port',
        search: 'Rechercher',
        selected: 'Sélectionné',
        clear: 'Effacer',
        finish: 'Terminer',
        next: 'Suivant',
        previous: 'Précédent',
        apply: 'Appliquer',
        defaults: 'Par défaut',
        unmanaged: 'Non géré',
        readOnly: 'Lecture seule',
        rename: 'Renommer',
        password: 'Mot de passe',
        currentPassword: 'Mot de passe actuel',
        newPassword: 'Nouveau mot de passe',
        confirmPassword: 'Confirmer le mot de passe',
        passwordPrompt: 'Entrez un mot de passe',
        passwordWeak: 'Faible',
        passwordMedium: 'Moyen',
        passwordStrong: 'Fort',
        savePassword: 'Enregistrer le mot de passe',
        editName: 'Modifier le nom',
        state: 'État',
        name: 'Nom',
        message: 'Message',
        timestamp: 'Horodatage',
        overview: 'Aperçu',
        login: 'Connexion',
        logout: 'Déconnexion',
        temperature: 'Température',
        duty: 'Puissance',
        offset: 'Décalage',
        stay: 'Rester',
        discard: 'Abandonner',
        blankNameResetDefault: 'Un nom vide réinitialisera à la valeur système par défaut.',
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
            applicationInfo: 'Info & Outils',
            back: 'Retour',
            expandMenu: 'Développer le menu',
            collapseMenu: 'Réduire le menu',
            controls: 'Contrôles',
            alerts: 'Alertes',
            settings: 'Paramètres',
            openInBrowser: 'Ouvrir dans le navigateur',
            modes: 'Modes',
            loginSuccessful: 'Connexion réussie',
        },
        settings: {
            title: 'Paramètres',
            userInterface: 'Interface utilisateur',
            device: 'Appareils',
            daemon: 'Daemon',
            devices: {
                devicesAndSensors: 'Appareils et capteurs',
                detectionIssues: 'Problèmes de détection ? Consultez la',
                hardwareSupportDoc: 'documentation de support matériel',
                toggleRequiresRestart:
                    "La modification des appareils ou des capteurs nécessite un redémarrage du daemon et de l'interface. Voulez-vous le faire maintenant ?",
                enableDevices: 'Activer les appareils',
                selectTooltip:
                    'Sélectionnez les appareils et capteurs à désactiver ou activer.\nIl est fortement recommandé de désactiver les appareils et capteurs inutilisés.',
                unknownError:
                    "Erreur inconnue lors de la tentative d'application des modifications à tous les appareils. Consultez les journaux pour plus de détails.",
            },
            plugin: 'Plugins (bêta)',
            plugins: {
                device: 'Plugin de service de périphérique',
                integration: "Plugin d'intégration",
                privileged: 'Accès privilégié',
                pluginUrl: "Page d'accueil",
                restricted: 'Accès restreint',
                settingsSaved: 'Paramètres du plugin enregistrés avec succès',
                settingsNotSaved: "Échec de l'enregistrement des paramètres du plugin",
            },
            profiles: 'Profils',
            alerts: 'Alertes',
            dashboards: 'Tableaux de bord',
            modes: 'Modes',
            appearance: 'Apparence',
            language: 'Langue',
            selectLanguage: 'Sélectionner la langue',
            english: 'Anglais',
            chinese: 'Chinois (simplifié)',
            japanese: 'Japonais',
            chineseTrad: 'Chinois (traditionnel)',
            russian: 'Russe',
            german: 'Allemand',
            french: 'Français',
            spanish: 'Espagnol',
            arabic: 'Arabe',
            portuguese: 'Portugais',
            hindi: 'Hindi',
            theme: 'Thème',
            themeLight: 'Clair',
            themeDark: 'Sombre',
            themeSystem: 'Système',
            themeCustom: 'Personnalisé',
            themeHighContrastDark: 'Sombre à haut contraste',
            themeHighContrastLight: 'Clair à haut contraste',
            lineThickness: 'Épaisseur des lignes',
            fullScreen: 'Plein écran',
            menuBarAlwaysVisible: 'Barre de menu toujours visible',
            hideMenuCollapseIcon: "Masquer l'icône de réduction du menu",
            eyeCandy: 'Effets visuels',
            showOnboarding: 'Afficher le guide au démarrage',
            introduction: 'Introduction',
            startTour: 'Démarrer la visite',
            timeFormat: "Format de l'heure",
            time24h: '24 heures',
            time12h: '12 heures',
            frequencyPrecision: 'Précision de la fréquence',
            startupPage: 'Page de démarrage',
            sidebarToCollapse: 'Barre latérale à réduire',
            entitiesBelowSensors: 'Entités sous les capteurs',
            dashboardLineSize: 'Taille des lignes du tableau de bord',
            themeStyle: 'Style du thème',
            themeMode: {
                system: 'Système',
                dark: 'Sombre',
                light: 'Clair',
                highContrastDark: 'Sombre à haut contraste',
                highContrastLight: 'Clair à haut contraste',
                custom: 'Personnalisé',
            },
            desktop: 'Bureau',
            startInTray: "Démarrer dans la barre d'état",
            closeToTray: "Réduire dans la barre d'état",
            zoom: 'Zoom',
            desktopStartupDelay: 'Délai de démarrage du bureau',
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
            deviceListener: "Surveillance des changements d'appareils",
            customTheme: {
                title: 'Thème Personnalisé',
                accent: "Couleur d'Accent",
                bgOne: 'Fond Principal',
                bgTwo: 'Fond Secondaire',
                border: 'Couleur de la Bordure',
                text: 'Couleur du Texte',
                textSecondary: 'Couleur du Texte Secondaire',
                export: 'Exporter le Thème',
                import: 'Importer le Thème',
                copyCode: 'Copier le Code',
                pasteCode: 'Coller le Code',
                themeCodeCopied: 'Code du thème copié',
                themeApplied: 'Thème appliqué',
                invalidThemeCode: 'Code de thème invalide',
            },
            tooltips: {
                introduction: "Commencer le tour d'introduction de l'application.",
                timeFormat: "Format de l'heure : 12 heures (AM/PM) ou 24 heures",
                frequencyPrecision: 'Ajuster la précision des valeurs de fréquence affichées.',
                startupPage: "La page affichée après le chargement de l'application.",
                eyeCandy:
                    'Activer les animations visuelles comme les icônes de ventilateurs en rotation.\nCela utilisera des ressources GPU supplémentaires.',
                sidebarCollapse:
                    'Afficher ou non une icône de réduction du menu dans la barre latérale,\nou utiliser la zone vide de la barre latérale pour étendre ou réduire le menu principal.',
                entitiesBelowSensors:
                    "Afficher ou non les entités sous les capteurs de l'appareil dans le menu principal.",
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
                deviceListener:
                    "Surveiller les événements d'ajout/suppression d'appareils (ex. branchement USB)\net notifier lorsque des changements matériels sont détectés.",
                daemonAddress:
                    "L'adresse IP ou le nom de domaine du daemon pour établir une connexion.\nPrend en charge IPv4, IPv6 et les noms d'hôte résolvables par DNS.",
                daemonPort: 'Le port utilisé pour établir une connexion avec le daemon.',
                sslTls: 'Se connecter au daemon en utilisant SSL/TLS.\nUne configuration de proxy est requise.',
                triggersRestart: 'Déclenche un redémarrage automatique',
                triggersUIRestart:
                    "Déclenche un redémarrage automatique de l'interface utilisateur",
                triggersDaemonRestart: 'Déclenche un redémarrage automatique du daemon',
                resetToDefaults: 'Réinitialiser aux paramètres par défaut',
                saveAndReload: "Enregistrer et recharger l'interface utilisateur",
                copyThemeCode:
                    'Copier un code compact représentant votre thème personnalisé actuel.\nPartagez-le dans des chats ou forums.',
                pasteThemeCode:
                    "Appliquer un thème personnalisé depuis un code (cct1:...) qu'on vous a partagé.",
                exportThemeFile: 'Enregistrer le thème personnalisé actuel dans un fichier JSON.',
                importThemeFile: 'Charger un thème personnalisé depuis un fichier JSON.',
            },
            applySettingAndRestart:
                "Changer ce paramètre nécessite un redémarrage du daemon et de l'interface utilisateur. Êtes-vous sûr de vouloir le faire maintenant?",
            restartHeader: 'Appliquer le paramètre et redémarrer',
            restartSuccess: 'Redémarrage en cours',
            success: 'Succès',
            successDetail: 'Opération terminée avec succès',
            settingsAppliedSuccess: 'Paramètres appliqués avec succès',
            restartRequestSuccess: 'Demande de redémarrage envoyée avec succès',
            colorPickerDialogTitle: 'Sélectionner la couleur',
            colorPickerConfirm: 'Confirmer',
            colorPickerCancel: 'Annuler',
            languageChangeConfirm: 'Changer de langue ?',
            languageChangeConfirmMessage:
                "Êtes-vous sûr de vouloir continuer ? Si certains éléments de l'interface ne s'affichent pas correctement, veuillez actualiser la page manuellement.",
            languageChangeSuccess: 'Langue changée avec succès.',
            languageChangeError: 'Échec du changement de langue. Veuillez réessayer.',
            themeChangeSuccess: 'Thème changé avec succès.',
            entitiesBelowSensorsEnabledMessage:
                'Les entités seront désormais affichées sous les capteurs.',
            entitiesBelowSensorsDisabledMessage:
                'Les entités ne seront plus affichées sous les capteurs.',
        },
        menu: {
            system: 'Système',
            dashboards: 'Tableaux de bord',
            profiles: 'Profils',
            functions: 'Fonctions',
            customSensors: 'Capteurs personnalisés',
            modes: 'Modes',
            alerts: 'Alertes',
            pinned: 'Épinglé',
            tooltips: {
                delete: 'Supprimer',
                createMode: 'Créer un mode à partir des paramètres actuels',
                addProfile: 'Ajouter un profil',
                editName: 'Modifier le nom',
                addAlert: 'Ajouter une alerte',
                deleteFunction: 'Supprimer la fonction',
                addDashboard: 'Ajouter un tableau de bord',
                deleteDashboard: 'Supprimer le tableau de bord',
                duplicate: 'Dupliquer',
                setAsHome: 'Définir comme accueil',
                save: 'Enregistrer',
                deleteMode: 'Supprimer le mode',
                updateWithCurrentSettings: 'Mettre à jour avec les paramètres actuels',
                rename: 'Renommer',
                createModeFromCurrentSettings: 'Créer un mode à partir des paramètres actuels',
                addCustomSensor: 'Ajouter un capteur personnalisé',
                addFunction: 'Ajouter une fonction',
                chooseColor: 'Choisir une couleur',
                deviceSettings: 'Paramètres Avancés du Périphérique',
                options: "Plus d'Options",
                seeDeviceHealth:
                    "Pour une vue d'ensemble, voir État des Périphériques dans Info & Outils",
                moveTop: 'Déplacer en Haut',
                moveBottom: 'Déplacer en Bas',
                disable: 'Désactiver',
                pin: 'Épingler en Haut',
                unpin: 'Désépingler',
                profileApply: 'Appliquer le Profil aux ventilateurs',
                tags: 'Gérer les Tags',
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
            docsLink: 'Documentation des Plugins',
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
            disabled: 'Désactivé',
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
            dashboard: 'Tableau de bord',
            mode: 'Mode',
            profile: 'Profil',
            function: 'Fonction',
            alert: 'Alerte',
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
            thinkpadFanControl: 'Contrôle du Ventilateur ThinkPad',
            pollRate: 'Taux de Sondage',
            applySettingAndRestart: 'Appliquer le Paramètre et Redémarrer',
            changeSetting:
                "Modifier ce paramètre nécessite un redémarrage du daemon et de l'interface. Êtes-vous sûr de vouloir le faire maintenant ?",
            status: {
                ok: 'Ok',
                hasWarnings: 'A des Avertissements',
                hasErrors: 'A des Erreurs',
            },
        },
        devices: {
            detectionIssues: 'Problèmes de détection ? Consultez la',
            hardwareSupportDocs: 'Documentation de Support Matériel',
            selectDevices:
                'Sélectionnez les appareils et capteurs à désactiver ou activer.\nIl est fortement recommandé de désactiver les appareils et capteurs inutilisés.',
            devicesAndSensors: 'Appareils et Capteurs',
            apply: 'Appliquer',
            applySettingsAndReload: 'Appliquer les paramètres et recharger',
            triggersAutoRestart: 'Déclenche le redémarrage automatique',
            restartPrompt:
                "L'activation ou la désactivation des appareils ou des capteurs nécessite un redémarrage du daemon et de l'interface. Êtes-vous sûr de vouloir le faire maintenant ?",
            enableDevices: 'Activer les Appareils',
        },
        speed: {
            automatic: 'Automatique',
            manual: 'Manuel',
            unsavedChanges: 'Changements non enregistrés',
            unsavedChangesMessage:
                'Il y a des changements non enregistrés apportés à ce canal de contrôle.',
            manualDuty: 'Cycle Manuel',
            profileToApply: 'Profil à appliquer',
            controlModeAutomaticTooltip: 'Appliquer un Profil de ventilateur à ce canal',
            controlModeManualTooltip: 'Définir un cycle de service fixe manuellement',
            controlModeUnmanagedTooltip:
                "Ne plus gérer ce canal, laissant le matériel ou pilote de l'appareil décider",
            driverNoSupportControl:
                'Canal en lecture seule. Le pilote actuel ne prend pas en charge le réglage de la vitesse de ce canal.',
            amdOverdriveNotEnabled:
                "AMD GPU overdrive n'est pas activé. Activez-le dans les paramètres avancés de cet appareil (redémarrage requis).",
            controlOrView: 'Contrôler ou Afficher',
            applySetting: 'Appliquer le Paramètre',
            defaultProfileInfo:
                "Sélectionner « Non géré » indique à CoolerControl d'arrêter de contrôler<br/>ce ventilateur et de rendre le contrôle au pilote du périphérique.<br/><br/><b>Attention :</b> De nombreux pilotes ne disposent <i>pas</i> d'un contrôle<br/>automatique du ventilateur. Sur ces appareils, le ventilateur restera<br/>à sa dernière vitesse définie.",
        },
        customSensors: {
            missingSourcesNotice:
                'Les sources de température suivantes ne sont plus présentes et seront supprimées lors de la sauvegarde: {sources}',
            newSensor: 'Nouveau Capteur',
            sensorType: 'Type de Capteur',
            type: 'Type',
            mixFunction: 'Fonction de Mélange',
            howCalculateValue: 'Comment calculer la valeur résultante du capteur',
            tempFileLocation: 'Emplacement du Fichier de Température',
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
            tempWeightsTooltip: 'Le poids individuel de chaque source de température sélectionnée.',
            tempName: 'Nom de la Température',
            weight: 'Poids',
            saveSensor: 'Enregistrer le Capteur',
            saveCustomSensor: 'Enregistrer le Capteur Personnalisé',
            unsavedChanges:
                'Il y a des changements non enregistrés apportés à ce Capteur Personnalisé.',
            unsavedChangesHeader: 'Changements non enregistrés',
            stay: 'Rester',
            discard: 'Abandonner',
            selectCustomSensorFile: 'Sélectionner un Fichier de Capteur Personnalisé',
            deleteCustomSensor: 'Supprimer le Capteur Personnalisé',
            deleteCustomSensorConfirm:
                'Êtes-vous sûr de vouloir supprimer le capteur personnalisé : "{name}" ?',
        },
        dashboard: {
            timeRange: 'Plage de Temps',
            minutes: 'min',
            chartType: 'Type de Graphique',
            dataType: 'Type de Données',
            filterSensors: 'Filtrer les Capteurs',
            showControls: 'Afficher les Contrôles',
            mouseActions:
                "Actions de la souris sur le tableau de bord :\n- Mettre en surbrillance la sélection pour zoomer.\n- Faire défiler pour zoomer.\n- Cliquer avec le bouton droit pour faire glisser lorsque zoomé.\n- Double-cliquer pour réinitialiser et reprendre la mise à jour.\n- Ctrl+cliquer ou cliquer avec le bouton du milieu pour afficher tous les capteurs dans l'info-bulle.",
            fullPage: 'Pleine Page',
            filterTags: 'Filtrer les Tags',
            filterByTag: 'Filtrer par Tag',
            filterBySensor: 'Filtrer par Capteur',
            search: 'Rechercher',
            filterTypes: 'Filtrer les Types',
            filterByDataType: 'Filtrer par Type de Données',
            selectChartType: 'Sélectionner un Type de Graphique',
            exitFullPage: 'Quitter la Pleine Page',
            controls: 'Contrôles',
            sensorValues: 'Valeurs des Capteurs',
            selected: 'Sélectionné',
            clear: 'Effacer',
            deleteDashboard: 'Supprimer le Tableau de Bord',
            deleteDashboardConfirm:
                'Êtes-vous sûr de vouloir supprimer le tableau de bord : "{name}" ?',
            dashboardDeleted: 'Tableau de Bord Supprimé',
            setAsHome: 'Définir comme Accueil',
            duplicateDashboard: 'Dupliquer le Tableau de Bord',
        },
        appInfo: {
            gettingStartedAutoCreate:
                '{wizard} permet de configurer des profils de base pour tous vos ventilateurs en une seule fois.',
            gettingStartedAutoCreateLink: 'Créer des profils automatiquement',
            calibrateFans:
                "Pour un contrôle cohérent, {wizard} afin qu'un % donné corresponde à une vitesse similaire sur chaque ventilateur.",
            calibrateFansLink: 'étalonnez vos ventilateurs',
            title: 'Info & Outils',
            noWarranty: 'Ce programme est fourni sans absolument aucune garantie.',
            changeStartupPage: 'Modifier la page de démarrage dans les paramètres',
            daemonStatus: 'État du Daemon',
            acknowledgeIssues: 'Reconnaître les Problèmes',
            status: 'État',
            processStatus: 'État du Processus',
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
            uiTourDesc: "Faites une visite guidée de l'application",
            gettingStarted: 'Premiers Pas',
            gettingStartedGraphProfile: 'Profil graphique',
            gettingStartedControlsPage: 'page Contrôles',
            gettingStartedStep1:
                'Créez un {profile} dans les Profils de ventilateur et façonnez la courbe.',
            gettingStartedStep2:
                'Assignez-le sur la {controls} (ou la page du ventilateur). Les Profils ne sont pas appliqués automatiquement.',
            gettingStartedStep3:
                'Réutilisez le même Profil sur autant de ventilateurs que vous le souhaitez.',
            helpSettingUp: 'Aide à la configuration du contrôle des ventilateurs',
            hardwareSupport: 'Support Matériel',
            hardwareSupportDesc: 'Appareils pris en charge et installation des pilotes',
            gitRepository: 'Dépôt Git',
            gitRepositoryDesc: 'Signaler des problèmes ou demander des fonctionnalités',
            discord: 'Discord',
            discordDesc: 'Rejoignez notre communauté Discord',
            whatsNew: 'Nouveautés',
            whatsNewDesc: 'Voir les dernières notes de version',
            logsAndDiagnostics: 'Journaux et Diagnostics',
            downloadCurrentLog: 'Télécharger le Journal Actuel',
            deviceHealth: 'État des Périphériques',
            deviceHealthTooltip:
                'Les sources de température manquantes peuvent être remplacées en modifiant<br>et en enregistrant à nouveau le Capteur personnalisé, le Profil ou le réglage LCD concerné.',
            deviceHealthOk: 'Tous les capteurs et canaux fonctionnent correctement.',
            failsafeActive: 'Valeurs de secours utilisées',
            missingTempSource: 'Source de température manquante',
            stressTest: 'Tests de stress thermique',
            stressTestTooltip:
                'Génère une charge thermique soutenue pour valider<br>les courbes de ventilateur et les profils de refroidissement.<br>Les résultats peuvent varier selon le matériel.<br>Installez stress-ng pour des backends supplémentaires.',
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
            threadCount: 'Threads',
            duration: 'Durée (s)',
            start: 'Démarrer',
            stop: 'Arrêter',
            stopAll: 'Tout arrêter',
            active: 'Actif',
            inactive: 'Inactif',
            allCores: 'Tous les cœurs',
            psuWarningHeader: 'Avertissement: consommation élevée',
            psuWarningMessage:
                "L'exécution simultanée des tests de stress CPU et GPU sollicitera fortement votre alimentation. En cas d'overclocking ou avec une alimentation de faible puissance, une instabilité système peut survenir. Voulez-vous continuer ?",
            proceed: 'Continuer',
        },
        alerts: {
            createAlert: 'Créer une Alerte',
            editAlert: "Modifier l'Alerte",
            deleteAlert: "Supprimer l'Alerte",
            noAlerts: 'Aucune alerte configurée',
            alertsOverview: 'Aperçu des Alertes',
            alertLogs: "Journaux d'Alertes",
            alertTriggered: 'Alerte Déclenchée',
            alertRecovered: 'Alerte Récupérée',
            deleteAlertConfirm: 'Êtes-vous sûr de vouloir supprimer : "{name}" ?',
            saveAlert: "Enregistrer l'Alerte",
            channelSource: "Source de Canal pour l'Alerte",
            channelSourceTooltip: "La source de canal à utiliser pour l'Alerte",
            triggerConditions: 'Conditions de Déclenchement',
            maxValueTooltip: "Les valeurs au-dessus de ceci déclencheront l'alerte.",
            minValueTooltip: "Les valeurs en dessous de ceci déclencheront l'alerte.",
            warmupDurationTooltip:
                "Durée pendant laquelle une condition doit être active avant que l'alerte soit considérée comme active.\nCette durée est vérifiée uniquement à intervalles réguliers\net peut donc varier.",
            greaterThan: 'supérieur à',
            lessThan: 'inférieur à',
            newAlert: 'Nouvelle Alerte',
            warmupGreaterThan: 'condition déclenchée plus longtemps que',
            unsavedChanges: 'Il y a des changements non enregistrés apportés à cette Alerte.',
            unsavedChangesHeader: 'Changements non enregistrés',
            createFailAlert: 'Alerte de panne',
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
            createProfile: 'Créer un profil',
            editProfile: 'Modifier le profil',
            deleteProfile: 'Supprimer le profil',
            noProfiles: 'Aucun profil configuré',
            profileType: 'Type de profil',
            fixedDuty: 'Vitesse de ventilateur fixe',
            selectedPointDuty: 'Puissance du point sélectionné',
            selectedPointTemp: 'Température du point sélectionné',
            tempSource: 'Source de température',
            memberProfiles: 'Profils membres',
            mixFunction: 'Fonction de mixage',
            applyMixFunction: 'Appliquer la fonction de mixage aux profils sélectionnés',
            profilesToMix: 'Profils à mixer',
            saveProfile: 'Enregistrer le profil',
            function: 'Fonction',
            functionToApply: 'Fonction à appliquer',
            graphProfileMouseActions:
                'Actions de la souris pour le profil graphique :\n- Défilement pour zoomer.\n- Clic gauche sur la ligne pour ajouter un point.\n- Clic droit sur un point pour le supprimer.\n- Glisser-déposer pour déplacer un point.',
            unsavedChanges: 'Des modifications non enregistrées ont été apportées à ce profil.',
            unsavedChangesHeader: 'Modifications non enregistrées',
            appliedFunction: 'Fonction appliquée',
            newProfile: 'Nouveau profil',
            tooltip: {
                profileType:
                    "Types de profils:<br/>- Par défaut: Non géré, rend le contrôle au pilote du périphérique<br/>- Fixe: définit une vitesse constante<br/>- Graphique: courbe de ventilateur personnalisable<br/>- Mélange: combine plusieurs profils<br/>- Superposition: applique un décalage à la sortie d'un profil existant",
            },
            profileDeleted: 'Profil supprimé',
            profileDuplicated: 'Profil dupliqué',
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
            selectedPointOutputDuty: 'Puissance de sortie du profil au point sélectionné',
            selectedPointOffset: 'Puissance de décalage du point sélectionné',
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
        controls: {
            viewType: 'Type de Vue',
            controlOrView: 'Contrôler ou Afficher',
            title: 'Contrôles du Système',
            noControllableChannels: 'Aucun canal contrôlable trouvé.',
            noControlChain: 'Aucune chaîne de contrôle trouvée pour ce canal.',
            controlFlow: 'Flux de Contrôle',
            backToOverview: 'Retour à la vue des contrôles',
            switchProfile: 'Changer de profil',
            switchTempSource: 'Changer la source de température',
            switchFunction: 'Changer la fonction',
            switchMembers: 'Changer les profils membres',
            switchBaseProfile: 'Changer le profil de base',
            adjustFixedSpeed: 'Ajuster la vitesse fixe',
            editSources: 'Modifier les sources',
        },
        modes: {
            createMode: 'Créer un Mode',
            editMode: 'Modifier le Mode',
            deleteMode: 'Supprimer le Mode',
            noModes: 'Aucun mode configuré',
            deleteModeConfirm: 'Êtes-vous sûr de vouloir supprimer le Mode : "{name}" ?',
            updateModeConfirm:
                'Êtes-vous sûr de vouloir écraser "{name}" avec la configuration actuelle ?',
            duplicateMode: 'Dupliquer le Mode',
        },
        functions: {
            createFunction: 'Créer une Fonction',
            editFunction: 'Modifier la Fonction',
            deleteFunction: 'Supprimer la Fonction',
            noFunctions: 'Aucune fonction configurée',
            saveFunction: 'Enregistrer la Fonction',
            functionType: 'Type de Fonction',
            functionTypeTooltip:
                'Types de fonction :<br/>' +
                '- <b>Identité</b> : Applique les limites de taille de pas mais transmet sinon la valeur du profil inchangée.<br/>' +
                "- <b>Standard</b> : Applique les limites de taille de pas et les paramètres d'hystérésis pour un contrôle précis du temps de réponse et de la stabilité du ventilateur.<br/>" +
                '- <b>Moyenne Mobile Exponentielle</b> : Lisse les fluctuations de température en utilisant une moyenne pondérée. Plus simple mais moins précis que Standard.',
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
            windowSize: 'Taille de la Fenêtre',
            windowSizeTooltip:
                "Taille de l'échantillon de température de fenêtre utilisée dans le calcul de la moyenne mobile exponentielle.\nValeurs plus petites = réponse plus rapide, plus réactif aux pics de température.\nValeurs plus grandes = réponse plus lente, transitions de vitesse du ventilateur plus douces.\nConseil : Utilisez une Fonction Standard pour un contrôle précis du temps de réponse.",
            emaCustomSensorAvailableNote:
                'EMA est également disponible en tant que type de Capteur Personnalisé, ce qui vous permet de tracer directement la température lissée.',
            emaDeprecatedWarning:
                'Le type de Fonction EMA est obsolète. Veuillez passer au type de Capteur Personnalisé EMA.',
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
            general: 'Général',
            thresholdHopping: 'Saut de Seuil',
            thresholdHoppingTooltip:
                "Lorsque la vitesse du ventilateur reste inchangée pendant 30+ secondes, les limites de taille de pas et d'hystérésis sont temporairement contournées.\nCela garantit que les ventilateurs atteignent finalement leur vitesse cible, même avec des paramètres de seuil conservateurs.",
            stepOverrides: 'Dérogations de pas',
            bypassMinAtExtremes: 'Toujours appliquer 0% / 100%',
            bypassMinAtExtremesTooltip:
                "Lorsque activé, les cycles cibles de 0% ou 100% sont appliqués même lorsque le changement est inférieur à la taille de pas minimale.\nUtile pour s'assurer que les ventilateurs s'arrêtent complètement ou atteignent leur RPM maximum. Désactivé par défaut.",
            unsavedChanges: 'Il y a des changements non enregistrés apportés à cette Fonction.',
            unsavedChangesHeader: 'Changements non enregistrés',
            functionError: 'Erreur lors de la tentative de mise à jour de cette fonction',
            newFunction: 'Nouvelle Fonction',
            functionDeleted: 'Fonction Supprimée',
            functionDuplicated: 'Fonction Dupliquée',
            deleteFunctionConfirm: 'Êtes-vous sûr de vouloir supprimer "{name}" ?',
            deleteFunctionWithProfilesConfirm:
                '"{name}" est actuellement utilisée par les Profils : {profiles}.\nLa suppression de cette Fonction réinitialisera les Fonctions de ces Profils.\nÊtes-vous sûr de vouloir supprimer "{name}" ?',
            functionUpdated: 'Fonction Mise à Jour',
            functionUpdateError:
                "Une erreur s'est produite lors de la tentative de mise à jour de cette Fonction",
        },
        error: {
            accessDenied: 'Accès Refusé',
            accessDeniedMessage:
                "L'authentification a échoué. Veuillez vérifier votre mot de passe et réessayer.",
            connectionError: 'Erreur de Connexion CoolerControl',
            connectionToast: 'Impossible de se connecter au daemon',
            connectionToastDetail:
                "Impossible de se connecter au daemon. Veuillez vous assurer que le service est en cours d'exécution et essayez de vous reconnecter.",
            connectionRetryFailure: 'Échec de connexion - nouvelle tentative échouée',
            connectionRetryDetail:
                'Impossible de se connecter au daemon après plusieurs tentatives.',
            errorLoadingGraph: 'Erreur lors du chargement du graphique',
            highCpuUsageWarning: 'Utilisation élevée du CPU détectée',
            highCpuUsageDetail:
                "L'utilisation actuelle du CPU est élevée.\nPour réduire l'impact sur le système, envisagez :\n1. De réduire le nombre de graphiques affichés\n2. De réduire le nombre de capteurs surveillés\n3. D'augmenter l'intervalle de sondage",
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
        singleDashboard: {
            minutes: 'min',
            chartMouseActions:
                'Actions de souris sur le tableau de bord :\n- Surligner pour zoomer.\n- Faire défiler pour zoomer.\n- Clic droit pour se déplacer lorsque zoomé.\n- Double-clic pour réinitialiser et reprendre la mise à jour.',
            timeRange: 'Plage de temps',
            chartType: 'Type de graphique',
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
        },
        shortcuts: {
            shortcuts: 'Raccourcis clavier',
            ctrl: 'Ctrl',
            alt: 'Alt',
            left: 'Gauche',
            right: 'Droite',
            comma: ',',
            h: 'h',
            a: 'a',
            c: 'c',
            i: 'i',
            slash: '/',
            one: '1',
            two: '2',
            three: '3',
            four: '4',
            f11: 'F11',
            viewShortcuts: 'Raccourcis clavier',
            home: "Page d'accueil",
            settings: 'Paramètres',
            info: "Informations sur l'application",
            dashboardOne: 'Tableau de bord 1',
            dashboardTwo: 'Tableau de bord 2',
            dashboardThree: 'Tableau de bord 3',
            dashboardFour: 'Tableau de bord 4',
            alerts: 'Alertes',
            controls: 'Contrôles',
            sideMenuCollapse: 'Réduire le menu latéral',
            sideMenuExpand: 'Développer le menu latéral',
            fullScreen: 'Plein écran',
        },
    },
    components: {
        confirmation: {
            title: 'Confirmation',
            message: 'Êtes-vous sûr ?',
        },
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
            title: 'Entrez Votre Mot de Passe',
            newPasswordTitle: 'Entrez Un Nouveau Mot de Passe',
            invalidPassword: 'Mot de Passe Invalide',
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
        helloWorld: {
            message:
                'Vous avez créé avec succès un projet avec Vite + Vue 3. Quelle est la suite ?',
        },
        dashboardInfo: {
            description:
                'Les tableaux de bord vous permettent de visualiser les données des capteurs de votre système selon vos préférences. Vous pouvez choisir entre des graphiques temporels ou tabulaires et ajuster les filtres et les paramètres de chaque graphique pour vous concentrer sur les données spécifiques que vous souhaitez voir. De plus, vous pouvez créer plusieurs tableaux de bord personnalisés pour répondre à vos besoins.',
        },
        modeInfo: {
            description:
                'Les modes vous permettent de sauvegarder les paramètres des canaux des appareils pour une application rapide et facile. Par exemple, vous pouvez créer un mode "Jeu" et un mode "Silencieux", vous permettant de basculer facilement entre eux.',
            note: "Veuillez noter que la création de différents profils de ventilateur peut être nécessaire pour chaque mode, car les modes n'incluent que les configurations de canal et n'englobent pas les paramètres internes de profil ou de fonction.",
        },
        alertInfo: {
            description:
                "Les alertes sont utilisées pour vous avertir lorsque des conditions spécifiques se produisent. Elles peuvent surveiller les températures et les vitesses des ventilateurs pour s'assurer que votre système fonctionne correctement. Les alertes sont configurées pour des plages de valeurs de capteur spécifiques et envoient des notifications lorsque les valeurs dépassent ou reviennent dans des plages de seuil acceptables.",
        },
        customSensorInfo: {
            title: 'Aperçu des Capteurs Personnalisés',
            description:
                'Les capteurs personnalisés vous permettent de combiner des capteurs existants de différentes manières, améliorant votre contrôle et votre efficacité sur le refroidissement du système. De plus, ils prennent en charge les données basées sur des fichiers, vous permettant de scripter des entrées de capteurs externes pour plus de flexibilité.',
            note: 'Remarque : Vous pouvez utiliser des profils de mélange pour combiner plusieurs sorties de capteurs personnalisés.',
        },
        functionInfo: {
            title: 'Aperçu des Fonctions',
            description:
                "Les fonctions sont des algorithmes configurables appliqués aux sorties de profil. Elles vous permettent de gérer quand les changements de vitesse des ventilateurs se produisent, d'ajuster les paramètres d'hystérésis et d'utiliser des moyennes mobiles pour les températures dynamiques.",
            identityFunction:
                "La fonction Identité est l'option la plus simple car elle ne modifie pas la sortie calculée du profil ; elle vous permet seulement de définir des plages minimales et maximales de changement de vitesse. Cela est particulièrement bénéfique pour minimiser les fluctuations constantes de vitesse des ventilateurs.",
        },
        profileInfo: {
            title: 'Aperçu des Profils',
            description:
                'Les profils définissent des paramètres personnalisables pour contrôler les vitesses des ventilateurs, le même profil pouvant être utilisé pour plusieurs ventilateurs. Les types incluent :',
            type: {
                fixed: 'Vitesse Fixe',
                fanCurve: 'Courbe de Ventilateur/Graphique',
                mix: 'Profil de Mélange',
                default: "Paramètres par Défaut de l'Appareil",
            },
            additionalInfo:
                "Les profils sont la base pour contrôler les vitesses des ventilateurs et peuvent être améliorés davantage en appliquant des fonctions d'algorithme plus avancées.",
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
            welcome: 'Bienvenue dans CoolerControl !',
            gettingStartedIntro:
                "Choisissez une visite pour vous orienter. La Visite Rapide couvre l'essentiel en quelques étapes. La Visite Complète vous guide à travers chaque menu et chaque bouton.",
            startTourAgain:
                'Vous pouvez recommencer cette visite à tout moment depuis la page Infos & Outils.',
            quickTour: 'Visite Rapide',
            thoroughTour: 'Visite Complète',
            maybeLater: 'Peut-être plus tard',
            openGettingStarted: 'Ouvrir la Documentation',
            finishLater: 'Je vais me débrouiller',
            appInfo: 'Infos & Outils',
            appInfoDesc:
                "Affiche les infos de l'app, le statut du daemon, les journaux, les liens utiles et les outils de test de charge. Un badge sur le logo vous alerte en cas de problème.",
            controls: 'Contrôles',
            controlsDesc:
                'Réglez les vitesses des ventilateurs, appliquez des Profils et gérez chaque canal détecté depuis un seul endroit.',
            profiles: 'Profils',
            profilesDesc:
                'Les Profils définissent comment un ventilateur réagit aux changements de température. Les Profils Graphiques permettent de dessiner des courbes personnalisées et peuvent être réutilisés sur plusieurs appareils.',
            functions: 'Fonctions',
            functionsDesc:
                'Les Fonctions sont appliquées aux Profils pour lisser les transitions de vitesse des ventilateurs et réduire le bruit.',
            systemMenu: 'Menu Système',
            systemMenuDesc:
                'Le menu principal liste les appareils et capteurs de ce système. Chaque section peut être développée pour afficher ses canaux et contrôles assignés.',
            dashboards: 'Tableaux de Bord',
            dashboardsDesc:
                'Les Tableaux de Bord permettent de créer des vues personnalisées avec des graphiques pour surveiller les températures, vitesses de ventilateurs et autres données de capteurs en temps réel.',
            modes: 'Modes',
            modesDesc:
                'Les Modes sont des collections enregistrées de vos paramètres. Basculez entre des configurations comme Silencieux et Performance en un clic.',
            alerts: 'Alertes',
            alertsDesc:
                'Les Alertes vous notifient lorsque les valeurs des capteurs dépassent les seuils que vous choisissez, vous permettant de réagir avant que des problèmes surviennent.',
            customSensors: 'Capteurs Personnalisés',
            customSensorsDesc:
                'Les Capteurs Personnalisés combinent les données existantes de différentes manières, ou exécutent votre propre script comme source de température.',
            quickAdd: 'Ajout Rapide',
            quickAddDesc:
                'Créez rapidement de nouveaux tableaux de bord, profils, fonctions et plus.',
            dashboardQuick: 'Menu Rapide du Tableau de Bord',
            dashboardQuickDesc:
                "Accédez à n'importe quel tableau de bord, même lorsque le menu principal est réduit.",
            modesQuick: 'Menu Rapide des Modes',
            modesQuickDesc:
                "Basculez entre vos Modes enregistrés depuis n'importe où dans l'application.",
            alertsQuick: 'Aperçu des Alertes',
            alertsQuickDesc:
                "Affichez l'état actuel de chaque alerte et inspectez leur activité récente.",
            pluginsQuick: 'Aperçu des Plugins',
            pluginsQuickDesc:
                "Parcourez les plugins installés et accédez à n'importe lequel depuis n'importe où dans l'application.",
            settings: 'Paramètres',
            settingsDesc:
                "Configurez les préférences de l'interface, les options du daemon et le comportement du système.",
            access: 'Accès',
            accessDesc: "Gérez votre mot de passe et confirmez votre niveau d'accès actuel.",
            restartMenu: 'Menu de Redémarrage',
            restartMenuDesc: "Rechargez l'interface ou redémarrez le daemon système si nécessaire.",
            collapseMenu: 'Réduire le Menu',
            collapseMenuDesc:
                "Développez ou réduisez le menu principal pour donner plus de place au reste de l'app.",
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
            min: 'Min',
            max: 'Max',
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
            newTag: 'Créer un Nouveau Tag',
            tagName: 'Nom du tag',
            editTag: 'Modifier le tag',
            deleteTag: 'Supprimer le tag',
        },
        wizards: {
            calibration: {
                title: 'Étalonner les ventilateurs',
                tooltip: 'Étalonner plusieurs ventilateurs pour un contrôle de vitesse cohérent',
                pickIntro:
                    'Sélectionnez les ventilateurs à étalonner. Les ventilateurs déjà étalonnés sont décochés par défaut.',
                noFans: 'Aucun ventilateur contrôlable détecté.',
                selectAll: 'Tout sélectionner',
                calibratedBadge: 'étalonné',
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
            generate: {
                title: 'Créer des profils automatiquement',
                tooltip:
                    'Créer automatiquement des profils pour vos ventilateurs à partir de quelques choix',
                stepFans: 'Attribuer les ventilateurs',
                stepTemps: 'Températures clés',
                stepPreset: 'Performance',
                assignIntro:
                    "Attribuez un rôle à chaque ventilateur. Laissez un ventilateur sans rôle pour l'ignorer.",
                skip: 'Ignorer',
                noFans: 'Aucun ventilateur contrôlable détecté.',
                tempsIntro:
                    'Confirmez vos températures clés. Elles sont pré-remplies comme meilleure estimation : veuillez les vérifier.',
                cpuTemp: 'Temp. CPU',
                gpuTemp: 'Temp. GPU',
                liquidTemp: 'Temp. du liquide',
                ambientTemp: 'Temp. ambiante (facultatif)',
                tempNone: 'Aucune',
                presetIntro: "Choisissez l'agressivité de la montée en régime des ventilateurs.",
                perKindOverrides: 'Remplacements par rôle (avancé)',
                cfmCaveat:
                    "Le biais de pression positive est basé sur le rapport cyclique (duty), pas sur le flux d'air : avec des nombres de ventilateurs déséquilibrés, il ne peut pas garantir une pression positive.",
                generate: 'Générer',
                preview: 'Aperçu',
                previewIntro:
                    "Vérifiez ce qui sera créé et appliqué. Rien n'est enregistré tant que vous n'avez pas confirmé.",
                previewAssignments: 'Attributions des ventilateurs',
                willCreateHeader: 'Sera créé',
                startingPointNote:
                    'Un point de départ simple plutôt que de partir de zéro. Ils ne seront pas parfaits pour chaque système, alors vérifiez-les, testez-les et ajustez-les après la création.',
                replaces: 'remplace {name}',
                createApply: 'Créer et appliquer',
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
                chooseFunctionNameType: 'Choisir un nom et un type de fonction',
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
            customSensor: {
                new: 'Nouveau Capteur Personnalisé',
            },
        },
        channelExtensionSettings: {
            title: "Paramètres du canal de l'appareil",
            firmwareControlledProfile: 'Profil contrôlé par le firmware',
            firmwareControlledProfileDesc:
                "Lorsque cette option est activée, le firmware de l'appareil gère le profil du ventilateur.\nUtile pour le matériel qui réagit mal aux modifications fréquentes de vitesse effectuées par le logiciel.\nDisponible uniquement pour les profils Graph qui utilisent des capteurs de température internes à l'appareil.\nLes paramètres de Fonction ne s'appliquent pas.",
            saveError: "Échec de l'enregistrement des paramètres de l'extension de canal",
            firmwareControlDisabled:
                "Le contrôle par firmware n'est pas disponible avec la configuration actuelle.\nUtilisez un profil Graph pour cet appareil avec un capteur de température interne pris en charge.",
            calibration: {
                heading: 'Étalonnage des RPM',
                description:
                    "Faites parcourir au ventilateur sa plage complète pour obtenir sa véritable courbe rapport cyclique/RPM, puis pilotez le canal en rapport cyclique réel normalisé par les RPM.\nSupprime les zones mortes à bas rapport cyclique et la saturation à haut rapport cyclique.\nLe coup d'envoi est aussi géré automatiquement quand le ventilateur est étalonné : une brève impulsion de démarrage le lance depuis l'arrêt avant qu'il ne se stabilise à la valeur cible.\nLe balayage prend généralement plusieurs minutes, et peut durer sensiblement plus longtemps pour des ventilateurs à réaction lente. Le canal est réglé à 0 % au début.",
                statusNotCalibrated: 'Non étalonné',
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
                stagePreflight: 'pré-vol',
                stageUpSweep: 'balayage montant',
                stageDownSweep: 'balayage descendant',
                stageFinalizing: 'finalisation',
                buttonCalibrate: 'Étalonner',
                buttonRecalibrate: 'Ré-étalonner',
                buttonCancel: 'Annuler',
                buttonClear: 'Effacer',
                buttonViewCurve: 'Voir la courbe',
                caveatsBanner:
                    "Étalonner plusieurs ventilateurs de refroidissement principaux en même temps peut faire monter la température du système.\nDes ventilateurs push-pull de radiateur diagnostiqués en parallèle peuvent produire des mesures inexactes.\nMaintenez le système au repos pendant l'étalonnage.",
                completedNotice:
                    'Étalonnage actif. Les courbes de ventilateur et les rapports cycliques manuels de ce canal pilotent désormais le rapport cyclique réel normalisé par les RPM. Vérifiez au besoin les valeurs de votre profil.',
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
            markerStart: 'Démarrage',
            markerSustain: 'Maintien',
            markerSaturate: 'Proche du plateau',
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
        loginFailed: 'Échec de Connexion',
        invalidPassword: 'Mot de Passe Invalide',
        passwordSetFailed: 'Échec de Définition du Mot de Passe',
        passwordSetSuccessfully: 'Nouveau mot de passe défini avec succès',
        logoutSuccessful: 'Vous vous êtes déconnecté avec succès.',
        unauthorizedAction: 'Vous devez être connecté pour effectuer cette action',
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
    device_store: {
        unauthorized: {
            summary: 'Session expirée',
            detail: 'Votre session a expiré. Rechargement pour se reconnecter.',
        },
        login: {
            success: {
                summary: 'Succès',
                detail: 'Connexion réussie.',
            },
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
            set_failed: {
                summary: 'Échec de Définition du Mot de Passe',
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
            functionType: {
                identity: 'Identité',
                standard: 'Standard',
                exponentialMovingAvg: 'Moyenne Mobile Exponentielle',
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
            lighting: 'Éclairage',
        },
    },
}

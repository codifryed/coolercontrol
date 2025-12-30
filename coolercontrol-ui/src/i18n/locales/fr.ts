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
        rename: 'Renommer',
        password: 'Mot de passe',
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
            restartUI: "Redémarrer l'interface",
            restartDaemonAndUI: "Redémarrer le daemon et l'interface",
            restartConfirmMessage: "Êtes-vous sûr de vouloir redémarrer le daemon et l'interface ?",
            restartConfirmHeader: 'Redémarrage du daemon',
            shutdownSuccess: "Signal d'arrêt du daemon accepté",
            shutdownError:
                "Erreur inconnue lors de l'envoi du signal d'arrêt. Consultez les journaux pour plus de détails.",
            quitDesktopApp: "Quitter l'application",
            applicationInfo: "Informations sur l'application",
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
            general: 'Général',
            device: 'Appareils',
            daemon: 'Daemon',
            thinkpad: 'ThinkPad',
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
            showOnboarding: 'Afficher le guide au démarrage',
            introduction: 'Introduction',
            startTour: 'Démarrer la visite',
            timeFormat: "Format de l'heure",
            time24h: '24 heures',
            time12h: '12 heures',
            frequencyPrecision: 'Précision de la fréquence',
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
            fanControl: 'Contrôle des ventilateurs',
            fullSpeed: 'Vitesse maximale',
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
            },
            tooltips: {
                introduction: "Commencer le tour d'introduction de l'application.",
                timeFormat: "Format de l'heure : 12 heures (AM/PM) ou 24 heures",
                frequencyPrecision: 'Ajuster la précision des valeurs de fréquence affichées.',
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
                thinkPadFanControl:
                    "Ceci est un assistant pour activer le contrôle du ventilateur ACPI de ThinkPad.\nLes opérations de contrôle du ventilateur sont désactivées par défaut pour des raisons de sécurité. CoolerControl peut essayer de l'activer pour vous, mais vous devez être conscient des risques pour votre matériel.\nProcédez à vos risques et périls.",
                thinkPadFullSpeed:
                    "Pour les ordinateurs portables ThinkPad, cela active le mode pleine vitesse.\nCela permet aux ventilateurs de tourner à leur maximum absolu lorsqu'ils sont réglés à 100 %, mais cela fera fonctionner les ventilateurs hors spécification et entraînera une usure accrue.\nUtilisez avec précaution.",
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
                moveTop: 'Déplacer en Haut',
                moveBottom: 'Déplacer en Bas',
                disable: 'Désactiver',
                pin: 'Épingler en Haut',
                unpin: 'Désépingler',
                profileApply: 'Appliquer le Profil aux ventilateurs',
            },
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
            automaticOrManual: 'Automatique ou Manuel',
            driverNoSupportControl:
                'Le pilote actuellement installé ne prend pas en charge le contrôle de ce canal.',
            controlOrView: 'Contrôler ou Afficher',
            applySetting: 'Appliquer le Paramètre',
            defaultProfileInfo:
                "Le profil par défaut restaure l'appareil à ses paramètres de pilote d'origine.<br/>Certains pilotes incluent un mode de contrôle automatique du ventilateur intégré, mais <i>beaucoup</i> ne le font pas.<br/>Pour les appareils sans contrôle automatique, l'application du profil par défaut laissera<br/>le ventilateur à sa dernière vitesse configurée et CoolerControl abandonnera le contrôle.",
        },
        customSensors: {
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
            title: "Informations sur l'Application",
            noWarranty: 'Ce programme est fourni sans absolument aucune garantie.',
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
            gettingStarted: 'Premiers Pas',
            helpSettingUp: 'Aide à la configuration du contrôle des ventilateurs',
            hardwareSupport: 'Support Matériel',
            hardwareSupportDesc: 'Appareils pris en charge et installation des pilotes',
            gitRepository: 'Dépôt Git',
            gitRepositoryDesc: 'Signaler des problèmes ou demander des fonctionnalités',
            discord: 'Discord',
            discordDesc: 'Rejoignez notre communauté Discord',
            logsAndDiagnostics: 'Journaux et Diagnostics',
            downloadCurrentLog: 'Télécharger le Journal Actuel',
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
                "Durée pendant laquelle une condition doit être active avant que l'alerte soit considérée comme active. Cette durée est vérifiée uniquement à intervalles réguliers et peut donc varier.",
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
                "Activer l'arrêt du système lorsque l'alerte est déclenchée.\nL'arrêt du système commencera une minute après le déclenchement de l'alerte et sera annulé si l'alerte récupère.",
        },
        profiles: {
            createProfile: 'Créer un profil',
            editProfile: 'Modifier le profil',
            deleteProfile: 'Supprimer le profil',
            noProfiles: 'Aucun profil configuré',
            systemDefault: 'Système par défaut',
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
                    "Types de profils:<br/>- Par défaut: conserve les paramètres actuels de l'appareil<br/>&nbsp;&nbsp;(BIOS/firmware)<br/>- Fixe: définit une vitesse constante<br/>- Graphique: courbe de ventilateur personnalisable<br/>- Mélange: combine plusieurs profils<br/>- Superposition: applique un décalage à la sortie d'un profil existant",
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
        },
        controls: {
            viewType: 'Type de Vue',
            controlOrView: 'Contrôler ou Afficher',
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
            passwordHelp:
                "Lors de l'installation, le daemon utilise un mot de passe par défaut pour protéger les points de contrôle des appareils. \nVous pouvez éventuellement créer un mot de passe fort pour une meilleure protection. \nSi vous voyez cette boîte de dialogue et que vous n'avez pas encore défini de mot de passe, essayez d'actualiser l'interface utilisateur \n ou cliquez sur Connexion dans le menu Protection d'Accès. Consultez le wiki du projet pour plus d'informations.",
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
            beforeStart: "Avant de commencer, l'une des choses les plus importantes à savoir est",
            settingUpDrivers: 'la configuration de vos pilotes matériels',
            fansNotShowing:
                "Si vos ventilateurs n'apparaissent pas ou ne peuvent pas être contrôlés, il y a probablement un problème avec les pilotes du noyau actuellement installés.",
            checkDocs:
                "Avant d'ouvrir un problème, veuillez confirmer que tous les pilotes ont été correctement chargés en",
            checkingDocs: 'consultant la documentation de Support Matériel',
            startTourAgain:
                'Remarque : vous pouvez recommencer cette visite à tout moment depuis la page des paramètres.',
            letsStart: "D'accord, commençons !",
            dashboards: 'Tableaux de Bord',
            dashboardsDesc:
                'Les tableaux de bord sont une collection organisée de graphiques pour visualiser les données des capteurs de votre système.',
            controls: 'Contrôles',
            controlsDesc:
                "Les contrôles offrent une interface interactive pour gérer les ventilateurs et autres appareils de votre système. Chaque canal contrôlable vous permet d'ajuster les vitesses, de définir des profils et de surveiller l'état en temps réel.",
            profiles: 'Profils',
            profilesDesc:
                'Les profils définissent des paramètres personnalisables pour contrôler les vitesses des ventilateurs. Le même profil peut être utilisé pour plusieurs ventilateurs et appareils.',
            functions: 'Fonctions',
            functionsDesc:
                "Les fonctions sont des algorithmes configurables qui peuvent être appliqués à la sortie d'un profil. Cela peut être utile pour gérer quand les changements de vitesse des ventilateurs se produisent.",
            appInfo: "Informations sur l'Application et le Daemon",
            appInfoDesc:
                "En cliquant sur le logo, vous ouvrez la page d'Informations sur l'Application, où vous pouvez obtenir des informations sur l'application, le daemon du système et les journaux. C'est là que vous devez aller lors du dépannage, et il y a un petit badge de statut du daemon ici pour vous informer de tout problème potentiel.",
            quickAdd: 'Ajout Rapide',
            quickAddDesc:
                "Il s'agit d'un menu pour ajouter facilement de nouveaux éléments comme des Tableaux de bord, des Profils, etc.",
            dashboardQuick: 'Menu Rapide du Tableau de Bord',
            dashboardQuickDesc:
                "Il s'agit d'un menu pour accéder rapidement à vos tableaux de bord, même si le menu principal est réduit.",
            settings: 'Paramètres',
            settingsDesc:
                "Ce bouton ouvrira la page des paramètres contenant différents paramètres d'interface utilisateur et de daemon.",
            restartMenu: 'Menu de Redémarrage',
            restartMenuDesc:
                "Ici, vous pouvez choisir de recharger l'interface utilisateur ou de redémarrer le daemon du système.",
            thatsIt: "C'est tout !",
            ready: "Et n'oubliez pas, si vos ventilateurs n'apparaissent pas ou ne peuvent pas être contrôlés, consultez la documentation de Support Matériel",
            startNow: "D'accord, vous êtes prêt à commencer !",
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
            average: 'Moyenne',
        },
        modeTable: {
            setting: 'Paramètre',
        },
        wizards: {
            fanControl: {
                fanControlWizard: 'Assistant de Contrôle des Ventilateurs',
                editCurrentProfile: 'Modifier le Profil Actuel',
                editCurrentFunction: 'Modifier la Fonction Actuelle',
                currentSettings: 'Voir les Paramètres Actuels',
                manualSpeed: 'Définir une Vitesse de Ventilateur Manuelle',
                createNewProfile: 'Créer un Nouveau Profil',
                existingProfile: 'Choisir un Profil Existant',
                resetSettings: 'Réinitialiser aux Paramètres par Défaut',
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
                    'Les fonctions vous permettent de contrôler davantage la façon dont la sortie du profil est appliquée.',
                createNewFunction: 'Créer une nouvelle fonction',
                existingFunction: 'Choisir une fonction existante',
                defaultFunction: 'Utiliser la fonction par défaut',
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
        },
    },
    auth: {
        enterPassword: 'Entrez Votre Mot de Passe',
        setNewPassword: 'Entrez Un Nouveau Mot de Passe',
        loginFailed: 'Échec de Connexion',
        invalidPassword: 'Mot de Passe Invalide',
        passwordSetFailed: 'Échec de Définition du Mot de Passe',
        passwordSetSuccessfully: 'Nouveau mot de passe défini avec succès',
        logoutSuccessful: 'Vous vous êtes déconnecté avec succès.',
        unauthorizedAction: 'Vous devez être connecté pour effectuer cette action',
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
            summary: 'Non Autorisé',
            detail: 'Vous devez être connecté pour effectuer cette action',
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
            controls: 'Contrôles',
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
            },
        },
        customSensor: {
            sensorType: {
                mix: 'Mélange',
                file: 'Fichier',
                offset: 'Décalage',
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
        alertState: {
            active: 'Actif',
            inactive: 'Inactif',
            error: 'Erreur',
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
    },
}

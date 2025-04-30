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
    secondAbbr: 's'
  },
  layout: {
    topbar: {
      login: 'Connexion',
      logout: 'Déconnexion',
      changePassword: 'Changer de mot de passe',
      restartUI: 'Redémarrer l\'interface',
      restartDaemonAndUI: 'Redémarrer le daemon et l\'interface',
      restartConfirmMessage: 'Êtes-vous sûr de vouloir redémarrer le daemon et l\'interface ?',
      restartConfirmHeader: 'Redémarrage du daemon',
      shutdownSuccess: 'Signal d\'arrêt du daemon accepté',
      shutdownError: 'Erreur inconnue lors de l\'envoi du signal d\'arrêt. Consultez les journaux pour plus de détails.',
      quitDesktopApp: 'Quitter l\'application',
      applicationInfo: 'Informations sur l\'application',
      back: 'Retour',
      expandMenu: 'Développer le menu',
      collapseMenu: 'Réduire le menu',
      alerts: 'Alertes',
      settings: 'Paramètres',
      openInBrowser: 'Ouvrir dans le navigateur',
      modes: 'Modes',
      loginSuccessful: 'Connexion réussie'
    },
    settings: {
      title: 'Paramètres',
      general: 'Général',
      device: 'Appareils et capteurs',
      devices: {
        devicesAndSensors: 'Appareils et capteurs',
        detectionIssues: 'Problèmes de détection ? Consultez la',
        hardwareSupportDoc: 'documentation de support matériel',
        toggleRequiresRestart: 'La modification des appareils ou des capteurs nécessite un redémarrage du daemon et de l\'interface. Voulez-vous le faire maintenant ?',
        enableDevices: 'Activer les appareils',
        selectTooltip: 'Sélectionnez les appareils et capteurs à désactiver ou activer.\nIl est fortement recommandé de désactiver les appareils et capteurs inutilisés.',
        unknownError: 'Erreur inconnue lors de la tentative d\'application des modifications à tous les appareils. Consultez les journaux pour plus de détails.'
      },
      profiles: 'Profils',
      alerts: 'Alertes',
      dashboards: 'Tableaux de bord',
      modes: 'Modes',
      appearance: 'Apparence',
      language: 'Langue',
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
      hideMenuCollapseIcon: 'Masquer l\'icône de réduction du menu',
      showOnboarding: 'Afficher le guide au démarrage',
      introduction: 'Introduction',
      startTour: 'Démarrer la visite',
      timeFormat: 'Format de l\'heure',
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
        custom: 'Personnalisé'
      },
      desktop: 'Bureau',
      startInTray: 'Démarrer dans la barre d\'état',
      closeToTray: 'Réduire dans la barre d\'état',
      zoom: 'Zoom',
      desktopStartupDelay: 'Délai de démarrage du bureau',
      fanControl: 'Contrôle des ventilateurs',
      fullSpeed: 'Vitesse maximale',
      applySettingsOnStartup: 'Appliquer les paramètres au démarrage',
      deviceDelayAtStartup: 'Délai avant de commencer la communication de l\'appareil (en secondes).\nAide avec les appareils qui prennent du temps à s\'initialiser ou sont détectés de manière intermittente',
      pollingRate: 'Le taux auquel les données du capteur sont interrogées (en secondes).\nUn taux d\'interrogation plus élevé réduira l\'utilisation des ressources, et un taux plus bas augmentera la réactivité.\nUn taux inférieur à 1,0 doit être utilisé avec précaution.',
      compressApiPayload: 'Activer la compression de la réponse pour réduire la taille de la charge utile de l\'API,\nmais notez que cela augmentera l\'utilisation du CPU.',
      liquidctlIntegration: 'Désactiver cela désactivera complètement l\'intégration de Liquidctl,\nindépendamment de l\'état d\'installation du package coolercontrol-liqctld. Si disponible, les pilotes HWMon seront utilisés à la place.',
      liquidctlDeviceInit: 'Attention : Désactivez cela UNIQUEMENT si vous, ou un autre programme, gérez l\'initialisation de l\'appareil liquidctl. Cela peut aider à éviter les conflits avec d\'autres programmes.',
      hideDuplicateDevices: 'Masquer les appareils en double',
      customTheme: {
        title: 'Thème Personnalisé',
        accent: 'Couleur d\'Accent',
        bgOne: 'Fond Principal',
        bgTwo: 'Fond Secondaire',
        border: 'Couleur de la Bordure',
        text: 'Couleur du Texte',
        textSecondary: 'Couleur du Texte Secondaire'
      },
      tooltips: {
        introduction: 'Commencer le tour d\'introduction de l\'application.',
        timeFormat: 'Format de l\'heure : 12 heures (AM/PM) ou 24 heures',
        frequencyPrecision: 'Ajuster la précision des valeurs de fréquence affichées.',
        sidebarCollapse: 'Afficher ou non une icône de réduction du menu dans la barre latérale,\nou utiliser la zone vide de la barre latérale pour étendre ou réduire le menu principal.',
        entitiesBelowSensors: 'Afficher ou non les entités sous les capteurs de l\'appareil dans le menu principal.',
        fullScreen: 'Basculer en mode plein écran',
        lineThickness: 'Ajuster l\'épaisseur des lignes des graphiques sur le tableau de bord',
        startInTray: 'Au démarrage, la fenêtre principale de l\'interface utilisateur sera masquée et seul\nle symbole de la barre d\'état système sera visible.',
        closeToTray: 'Fermer la fenêtre de l\'application laissera l\'application en cours d\'exécution dans la barre d\'état système',
        zoom: 'Définir manuellement le niveau de zoom de l\'interface utilisateur.',
        desktopStartupDelay: 'Ajoute un délai avant de démarrer l\'application de bureau (en secondes).\nAide à résoudre les problèmes qui surviennent lorsque l\'application de bureau\nest démarrée automatiquement à la connexion ou démarre trop rapidement',
        thinkpadFanControl: 'Ceci est un assistant pour activer le contrôle du ventilateur ACPI de ThinkPad.\nLes opérations de contrôle du ventilateur sont désactivées par défaut pour des raisons de sécurité. CoolerControl peut essayer de l\'activer pour vous, mais vous devez être conscient des risques pour votre matériel.\nProcédez à vos risques et périls.',
        thinkpadFullSpeed: 'Pour les ordinateurs portables Thinkpad, cela active le mode pleine vitesse.\nCela permet aux ventilateurs de tourner à leur maximum absolu lorsqu\'ils sont réglés à 100 %, mais cela fera fonctionner les ventilateurs hors spécification et entraînera une usure accrue.\nUtilisez avec précaution.',
        applySettingsOnStartup: 'Appliquer automatiquement les paramètres au démarrage du daemon et lors de la sortie de veille',
        deviceDelayAtStartup: 'Délai avant de commencer la communication de l\'appareil (en secondes).\nAide avec les appareils qui prennent du temps à s\'initialiser ou sont détectés de manière intermittente',
        pollingRate: "Le taux auquel les données du capteur sont interrogées (en secondes).\nUn taux d'interrogation plus élevé réduira l'utilisation des ressources, et un taux plus bas augmentera la réactivité.\nUn taux inférieur à 1,0 doit être utilisé avec précaution.",
        compressApiPayload: "Activer la compression de la réponse pour réduire la taille de la charge utile de l'API,\nmais notez que cela augmentera l'utilisation du CPU.",
        liquidctlIntegration: "Désactiver cela désactivera complètement l'intégration de Liquidctl,\nindépendamment de l'état d'installation du package coolercontrol-liqctld. Si disponible, les pilotes HWMon seront utilisés à la place.",
        liquidctlDeviceInit: "Attention : Désactivez cela UNIQUEMENT si vous, ou un autre programme, gérez l'initialisation de l'appareil liquidctl. Cela peut aider à éviter les conflits avec d'autres programmes.",
        hideDuplicateDevices: "Certains appareils sont pris en charge à la fois par les pilotes Liquidctl et HWMon. Liquidctl est utilisé par défaut pour ses fonctionnalités supplémentaires. Pour utiliser les pilotes HWMon à la place, désactivez cela et l'appareil liquidctl pour éviter les conflits de pilotes.",
        daemonAddress: 'L\'adresse IP ou le nom de domaine du daemon pour établir une connexion.\nPrend en charge IPv4, IPv6 et les noms d\'hôte résolvables par DNS.',
        daemonPort: 'Le port utilisé pour établir une connexion avec le daemon.',
        sslTls: 'Se connecter au daemon en utilisant SSL/TLS.\nUne configuration de proxy est requise.',
        triggersRestart: 'Déclenche un redémarrage automatique',
        triggersUIRestart: 'Déclenche un redémarrage automatique de l\'interface utilisateur',
        triggersDaemonRestart: 'Déclenche un redémarrage automatique du daemon',
        resetToDefaults: 'Réinitialiser aux paramètres par défaut',
        saveAndReload: 'Enregistrer et recharger l\'interface utilisateur',
        daemonSsl: 'Se connecter au daemon en utilisant SSL/TLS. Une configuration de proxy est requise.',
        applyOnBoot: 'Appliquer automatiquement les paramètres au démarrage du daemon et lors de la sortie de veille',
        startupDelay: 'Délai avant de commencer la communication de l\'appareil (en secondes). Aide avec les appareils qui prennent du temps à s\'initialiser ou sont détectés de manière intermittente',
        thinkPadFanControl: 'Ceci est un assistant pour activer le contrôle du ventilateur ACPI de ThinkPad. Les opérations de contrôle du ventilateur sont désactivées par défaut pour des raisons de sécurité. CoolerControl peut essayer de l\'activer pour vous, mais vous devez être conscient des risques pour votre matériel. Procédez à vos risques et périls.',
        thinkPadFullSpeed: 'Pour les ordinateurs portables Thinkpad, cela active le mode pleine vitesse. Cela permet aux ventilateurs de tourner à leur maximum absolu lorsqu\'ils sont réglés à 100 %, mais cela fera fonctionner les ventilateurs hors spécification et entraînera une usure accrue. Utilisez avec précaution.',
        compress: 'Activer la compression de la réponse pour réduire la taille de la charge utile de l\'API, mais notez que cela augmentera l\'utilisation du CPU.',
        liquidctlNoInit: "Attention : Désactivez cela UNIQUEMENT si vous, ou un autre programme, gérez l'initialisation de l'appareil liquidctl. Cela peut aider à éviter les conflits avec d'autres programmes.",
        hideDuplicate: "Certains appareils sont pris en charge à la fois par les pilotes Liquidctl et HWMon. Liquidctl est utilisé par défaut pour ses fonctionnalités supplémentaires. Pour utiliser les pilotes HWMon à la place, désactivez cela et l'appareil liquidctl pour éviter les conflits de pilotes.",
        liquidctl: "Désactiver cela désactivera complètement l'intégration de Liquidctl, indépendamment de l'état d'installation du package coolercontrol-liqctld. Si disponible, les pilotes HWMon seront utilisés à la place.",
        pollRate: "Le taux auquel les données du capteur sont interrogées (en secondes).\nUn taux d'interrogation plus élevé réduira l'utilisation des ressources, et un taux plus bas augmentera la réactivité.\nUn taux inférieur à 1,0 doit être utilisé avec précaution."
      },
      applySettingAndRestart: 'Changer ce paramètre nécessite un redémarrage du daemon et de l\'interface utilisateur. Êtes-vous sûr de vouloir le faire maintenant?',
      restartHeader: 'Appliquer le paramètre et redémarrer',
      restartSuccess: 'Redémarrage en cours',
      success: 'Succès',
      successDetail: 'Opération terminée avec succès',
      settingsAppliedSuccess: 'Paramètres appliqués avec succès',
      restartRequestSuccess: 'Demande de redémarrage envoyée avec succès',
      colorPickerDialogTitle: 'Sélectionner la couleur',
      colorPickerConfirm: 'Confirmer',
      colorPickerCancel: 'Annuler',
      languageChangeConfirm: 'Changer de langue?',
      languageChangeConfirmMessage: 'Changer de langue nécessite un rafraîchissement de la page. Continuer?'
    },
    menu: {
      system: 'Système',
      dashboards: 'Tableaux de bord',
      profiles: 'Profils',
      functions: 'Fonctions',
      customSensors: 'Capteurs personnalisés',
      modes: 'Modes',
      alerts: 'Alertes',
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
        chooseColor: 'Choisir une couleur'
      }
    },
    add: {
      dashboard: 'Tableau de bord',
      mode: 'Mode',
      profile: 'Profil',
      function: 'Fonction',
      alert: 'Alerte',
      customSensor: 'Capteur personnalisé'
    }
  },
  views: {
    daemon: {
      title: 'Daemon',
      daemonErrors: 'Erreurs du Daemon',
      daemonErrorsDetail: 'Le daemon a signalé des erreurs. Consultez les journaux pour plus de détails.',
      daemonDisconnected: 'Daemon Déconnecté',
      daemonDisconnectedDetail: 'Impossible de se connecter au daemon. Veuillez vérifier si le daemon est en cours d\'exécution.',
      connectionRestored: 'Connexion Rétablie',
      connectionRestoredMessage: 'La connexion au daemon a été rétablie.',
      thinkpadFanControl: 'Contrôle du Ventilateur ThinkPad',
      pollRate: 'Taux de Sondage',
      applySettingAndRestart: 'Appliquer le Paramètre et Redémarrer',
      changeSetting: 'Modifier ce paramètre nécessite un redémarrage du daemon et de l\'interface. Êtes-vous sûr de vouloir le faire maintenant ?',
      status: {
        ok: 'Ok',
        hasWarnings: 'A des Avertissements',
        hasErrors: 'A des Erreurs'
      }
    },
    devices: {
      detectionIssues: 'Problèmes de détection ? Consultez la',
      hardwareSupportDocs: 'Documentation de Support Matériel',
      selectDevices: 'Sélectionnez les appareils et capteurs à désactiver ou activer.\nIl est fortement recommandé de désactiver les appareils et capteurs inutilisés.',
      devicesAndSensors: 'Appareils et Capteurs',
      apply: 'Appliquer',
      applySettingsAndReload: 'Appliquer les paramètres et recharger',
      triggersAutoRestart: 'Déclenche le redémarrage automatique',
      restartPrompt: 'L\'activation ou la désactivation des appareils ou des capteurs nécessite un redémarrage du daemon et de l\'interface. Êtes-vous sûr de vouloir le faire maintenant ?',
      enableDevices: 'Activer les Appareils'
    },
    speed: {
      automatic: 'Automatique',
      manual: 'Manuel',
      unsavedChanges: 'Changements non enregistrés',
      unsavedChangesMessage: 'Il y a des changements non enregistrés apportés à ce canal de contrôle.',
      manualDuty: 'Cycle Manuel',
      profileToApply: 'Profil à appliquer',
      automaticOrManual: 'Automatique ou Manuel',
      driverNoSupportControl: 'Le pilote actuellement installé ne prend pas en charge le contrôle de ce canal.',
      controlOrView: 'Contrôler ou Afficher',
      applySetting: 'Appliquer le Paramètre'
    },
    customSensors: {
      newSensor: 'Nouveau Capteur',
      sensorType: 'Type de Capteur',
      type: 'Type',
      mixFunction: 'Fonction de Mélange',
      howCalculateValue: 'Comment calculer la valeur résultante du capteur',
      tempFileLocation: 'Emplacement du Fichier de Température',
      tempFile: 'Fichier de Température',
      filePathTooltip: 'Entrez le chemin absolu vers le fichier de température à utiliser pour ce capteur.\nLe fichier doit utiliser le format de données sysfs standard :\nUn nombre à virgule fixe en millidegrés Celsius.\np. ex. 80000 pour 80°C.\nLe fichier est vérifié lors de la soumission.',
      browse: 'Parcourir',
      browseCustomSensorFile: 'Parcourir pour un fichier de capteur personnalisé',
      tempSources: 'Sources de Température',
      tempSourcesTooltip: 'Sources de température à utiliser dans la fonction de mélange<br/><i>Remarque : Vous pouvez utiliser un Profil de Mélange pour combiner plusieurs<br/>Capteurs Personnalisés.</i>',
      tempWeights: 'Poids des Températures',
      tempWeightsTooltip: 'Le poids individuel de chaque source de température sélectionnée.',
      tempName: 'Nom de la Température',
      weight: 'Poids',
      saveSensor: 'Enregistrer le Capteur',
      saveCustomSensor: 'Enregistrer le Capteur Personnalisé',
      unsavedChanges: 'Il y a des changements non enregistrés apportés à ce Capteur Personnalisé.',
      unsavedChangesHeader: 'Changements non enregistrés',
      stay: 'Rester',
      discard: 'Abandonner',
      selectCustomSensorFile: 'Sélectionner un Fichier de Capteur Personnalisé',
      deleteCustomSensor: 'Supprimer le Capteur Personnalisé',
      deleteCustomSensorConfirm: 'Êtes-vous sûr de vouloir supprimer le capteur personnalisé : "{name}" ?'
    },
    dashboard: {
      timeRange: 'Plage de Temps',
      minutes: 'Minutes',
      chartType: 'Type de Graphique',
      dataType: 'Type de Données',
      filterSensors: 'Filtrer les Capteurs',
      showControls: 'Afficher les Contrôles',
      mouseActions: 'Actions de souris sur le tableau de bord :\n- Surligner pour zoomer.\n- Faire défiler pour zoomer.\n- Clic droit pour se déplacer lorsque zoomé.\n- Double-clic pour réinitialiser et reprendre la mise à jour.',
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
      deleteDashboardConfirm: 'Êtes-vous sûr de vouloir supprimer le tableau de bord : "{name}" ?',
      dashboardDeleted: 'Tableau de Bord Supprimé',
      setAsHome: 'Définir comme Accueil',
      duplicateDashboard: 'Dupliquer le Tableau de Bord'
    },
    appInfo: {
      title: 'Informations sur l\'Application',
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
      openIssue: 'Ouvrir un Problème',
      openIssueDesc: 'Signaler des bugs, demander des fonctionnalités',
      logsAndDiagnostics: 'Journaux et Diagnostics',
      downloadCurrentLog: 'Télécharger le Journal Actuel'
    },
    alerts: {
      createAlert: 'Créer une Alerte',
      editAlert: 'Modifier l\'Alerte',
      deleteAlert: 'Supprimer l\'Alerte',
      noAlerts: 'Aucune alerte configurée',
      alertsOverview: 'Aperçu des Alertes',
      alertLogs: 'Journaux d\'Alertes',
      alertTriggered: 'Alerte Déclenchée',
      alertRecovered: 'Alerte Récupérée',
      deleteAlertConfirm: 'Êtes-vous sûr de vouloir supprimer : "{name}" ?',
      saveAlert: 'Enregistrer l\'Alerte',
      channelSource: 'Source de Canal pour l\'Alerte',
      channelSourceTooltip: 'La source de canal à utiliser pour l\'Alerte',
      triggerConditions: 'Conditions de Déclenchement',
      maxValueTooltip: 'Les valeurs au-dessus de ceci déclencheront l\'alerte.',
      minValueTooltip: 'Les valeurs en dessous de ceci déclencheront l\'alerte.',
      greaterThan: 'supérieur à',
      lessThan: 'inférieur à',
      newAlert: 'Nouvelle Alerte',
      unsavedChanges: 'Il y a des changements non enregistrés apportés à cette Alerte.',
      unsavedChangesHeader: 'Changements non enregistrés'
    },
    profiles: {
      createProfile: 'Créer un Profil',
      editProfile: 'Modifier le Profil',
      deleteProfile: 'Supprimer le Profil',
      noProfiles: 'Aucun profil configuré',
      systemDefault: 'Système par Défaut',
      profileType: 'Type de Profil',
      fixedDuty: 'Vitesse de Ventilateur Fixe',
      selectedPointDuty: 'Cycle du Point Sélectionné',
      selectedPointTemp: 'Température du Point Sélectionné',
      tempSource: 'Source de Température',
      memberProfiles: 'Profils Membres',
      mixFunction: 'Fonction de Mélange',
      applyMixFunction: 'Appliquer la fonction de mélange aux profils sélectionnés',
      profilesToMix: 'Profils à mélanger',
      saveProfile: 'Enregistrer le Profil',
      function: 'Fonction',
      functionToApply: 'Fonction à appliquer',
      graphProfileMouseActions: 'Actions de souris pour le Profil Graphique :\n- Faire défiler pour zoomer.\n- Clic gauche sur la ligne pour ajouter un point.\n- Clic droit sur un point pour le supprimer.\n- Faire glisser un point pour le déplacer.',
      unsavedChanges: 'Il y a des changements non enregistrés apportés à ce Profil.',
      unsavedChangesHeader: 'Changements non enregistrés',
      appliedFunction: 'Fonction Appliquée',
      newProfile: 'Nouveau Profil',
      tooltip: {
        profileType: 'Type de Profil:<br/>- Par défaut: Conserve les paramètres actuels de l\'appareil<br/>&nbsp;&nbsp;(BIOS/firmware)<br/>- Fixe: Définit une vitesse constante<br/>- Graphique: Courbe de ventilateur personnalisable<br/>- Mélange: Combine plusieurs profils'
      },
      profileDeleted: 'Profil Supprimé',
      profileDuplicated: 'Profil Dupliqué',
      deleteProfileConfirm: 'Êtes-vous sûr de vouloir supprimer : "{name}" ?',
      deleteProfileWithChannelsConfirm: '"{name}" est actuellement utilisé par : {channels}.\nLa suppression de ce Profil réinitialisera les paramètres de ces canaux.\nÊtes-vous sûr de vouloir supprimer "{name}" ?',
      profileUpdated: 'Profil Mis à Jour',
      profileUpdateError: 'Une erreur s\'est produite lors de la tentative de mise à jour de ce Profil'
    },
    controls: {
      viewType: 'Type de Vue',
      controlOrView: 'Contrôler ou Afficher'
    },
    modes: {
      createMode: 'Créer un Mode',
      editMode: 'Modifier le Mode',
      deleteMode: 'Supprimer le Mode',
      noModes: 'Aucun mode configuré',
      deleteModeConfirm: 'Êtes-vous sûr de vouloir supprimer le Mode : "{name}" ?',
      updateModeConfirm: 'Êtes-vous sûr de vouloir écraser "{name}" avec la configuration actuelle ?',
      duplicateMode: 'Dupliquer le Mode'
    },
    functions: {
      createFunction: 'Créer une Fonction',
      editFunction: 'Modifier la Fonction',
      deleteFunction: 'Supprimer la Fonction',
      noFunctions: 'Aucune fonction configurée',
      saveFunction: 'Enregistrer la Fonction',
      functionType: 'Type de Fonction',
      functionTypeTooltip: 'Types de fonction :\n- Identité : Ne modifie pas la valeur calculée du profil.\n- Standard : Modifie la valeur du profil en utilisant un algorithme avec des paramètres d\'hystérésis.\n- Moyenne Mobile Exponentielle : Modifie la valeur du profil en utilisant un algorithme EMA.',
      minimumAdjustment: 'Ajustement Minimum',
      minimumAdjustmentTooltip: 'Ajustement minimal de la vitesse du ventilateur : Les changements calculés inférieurs à cette valeur seront ignorés.',
      maximumAdjustment: 'Ajustement Maximum',
      maximumAdjustmentTooltip: 'Ajustement maximal de la vitesse du ventilateur : Les changements calculés supérieurs à ce seuil seront plafonnés.',
      windowSize: 'Taille de la Fenêtre',
      windowSizeTooltip: 'Ajustez la sensibilité aux changements de température en définissant la taille de la fenêtre.\nLes tailles de fenêtre plus petites répondent rapidement aux changements,\ntandis que les tailles de fenêtre plus grandes fournissent des moyennes plus lisses.',
      hysteresisThreshold: 'Seuil d\'Hystérésis',
      hysteresisThresholdTooltip: 'Seuil de changement de température (°C) : Ajuster la vitesse du ventilateur lorsque la température change de cette quantité.',
      hysteresisDelay: 'Délai d\'Hystérésis',
      hysteresisDelayTooltip: 'Temps (secondes) nécessaire pour répondre aux changements de température.',
      onlyDownward: 'Seulement Descendant',
      onlyDownwardTooltip: 'Appliquer les paramètres uniquement lorsque la température diminue.',
      unsavedChanges: 'Il y a des changements non enregistrés apportés à cette Fonction.',
      unsavedChangesHeader: 'Changements non enregistrés',
      functionError: 'Erreur lors de la tentative de mise à jour de cette fonction',
      newFunction: 'Nouvelle Fonction',
      functionDeleted: 'Fonction Supprimée',
      functionDuplicated: 'Fonction Dupliquée',
      deleteFunctionConfirm: 'Êtes-vous sûr de vouloir supprimer "{name}" ?',
      deleteFunctionWithProfilesConfirm: '"{name}" est actuellement utilisée par les Profils : {profiles}.\nLa suppression de cette Fonction réinitialisera les Fonctions de ces Profils.\nÊtes-vous sûr de vouloir supprimer "{name}" ?',
      functionUpdated: 'Fonction Mise à Jour',
      functionUpdateError: 'Une erreur s\'est produite lors de la tentative de mise à jour de cette Fonction'
    },
    error: {
      connectionError: 'Erreur de Connexion CoolerControl',
      connectionToast: 'Impossible de se connecter au daemon',
      connectionToastDetail: 'Impossible de se connecter au daemon. Veuillez vous assurer que le service est en cours d\'exécution et essayez de vous reconnecter.',
      connectionRetryFailure: 'Échec de connexion - nouvelle tentative échouée',
      connectionRetryDetail: 'Impossible de se connecter au daemon après plusieurs tentatives.',
      errorLoadingGraph: 'Erreur lors du chargement du graphique',
      highCpuUsageWarning: 'Utilisation élevée du CPU détectée',
      highCpuUsageDetail: 'L\'utilisation actuelle du CPU est élevée.\nPour réduire l\'impact sur le système, envisagez :\n1. De réduire le nombre de graphiques affichés\n2. De réduire le nombre de capteurs surveillés\n3. D\'augmenter l\'intervalle de sondage',
      pageNotFound: 'Page Non Trouvée',
      returnToDashboard: 'Retour au Tableau de Bord',
      connectionErrorMessage: 'Impossible de se connecter au Daemon CoolerControl.',
      serviceRunningMessage: 'Veuillez vérifier si le service daemon est en cours d\'exécution.',
      checkProjectPage: 'Pour obtenir de l\'aide pour configurer le daemon, consultez la',
      projectPage: 'page du projet',
      helpfulCommands: 'Commandes utiles :',
      nonStandardAddress: 'Si vous avez une adresse de daemon non standard, vous pouvez la spécifier ci-dessous :',
      daemonAddressDesktop: 'Adresse du Daemon (Application de Bureau)',
      daemonAddressWeb: 'Adresse du Daemon (Interface Web)',
      addressTooltip: 'L\'adresse IP ou le nom de domaine pour établir une connexion.',
      portTooltip: 'Le port pour établir une connexion.',
      sslTooltip: 'Se connecter au daemon en utilisant SSL/TLS.',
      saveTooltip: 'Enregistrer les paramètres et recharger l\'interface utilisateur',
      resetTooltip: 'Réinitialiser aux paramètres par défaut'
    },
    singleDashboard: {
      minutes: "min",
      chartMouseActions: "Actions de souris sur le tableau de bord :\n- Surligner pour zoomer.\n- Faire défiler pour zoomer.\n- Clic droit pour se déplacer lorsque zoomé.\n- Double-clic pour réinitialiser et reprendre la mise à jour.",
      timeRange: "Plage de temps",
      chartType: "Type de graphique"
    },
    mode: {
      activateMode: "Activer le mode",
      currentlyActive: "Actuellement actif",
      modeHint: "Remarque : Les modes n'incluent pas les paramètres de Profil ou de Fonction, seulement les configurations de canal."
    },
    lighting: {
      saveLightingSettings: "Enregistrer les paramètres d'éclairage",
      lightingMode: "Mode d'éclairage",
      speed: "Vitesse",
      direction: "Direction",
      forward: "Avant",
      backward: "Arrière",
      numberOfColors: "Nombre de couleurs",
      numberOfColorsTooltip: "Nombre de couleurs à utiliser pour le mode d'éclairage choisi."
    },
    lcd: {
      saveLcdSettings: "Enregistrer les Paramètres LCD",
      lcdMode: "Mode LCD",
      brightness: "Luminosité",
      brightnessPercent: "Pourcentage de Luminosité",
      orientation: "Orientation",
      orientationDegrees: "Orientation en degrés",
      chooseImage: "Choisir une Image",
      dragAndDrop: "Glissez et déposez les fichiers ici.",
      tempSource: "Source de Température",
      tempSourceTooltip: "Source de température à utiliser dans l'affichage LCD.",
      imagesPath: "Chemin des Images",
      imagesPathTooltip: "Entrez le chemin absolu vers le répertoire contenant les images.\nLe répertoire doit contenir au moins un fichier image, et ils\npeuvent être des images statiques ou des gifs. Le Carrousel les parcourra\navec le délai sélectionné. Tous les fichiers sont traités\nlors de la soumission pour assurer une compatibilité maximale.",
      browse: "Parcourir",
      browseTooltip: "Parcourir pour un répertoire d'images",
      delayInterval: "Intervalle de Délai",
      delayIntervalTooltip: "Nombre minimum de secondes de délai entre les changements d'image.\nNotez que le délai réel peut être plus long en raison du taux de sondage du daemon.",
      processing: "Traitement en cours...",
      applying: "Application en cours...",
      unsavedChanges: "Il y a des changements non enregistrés apportés à ces Paramètres LCD.",
      unsavedChangesHeader: "Changements non enregistrés",
      imageTooLarge: "L'image est trop grande. Veuillez en choisir une plus petite.",
      notImageType: "L'image n'est pas reconnue comme un type d'image"
    }
  },
  components: {
    confirmation: {
      title: 'Confirmation',
      message: 'Êtes-vous sûr ?'
    },
    aseTek690: {
      sameDeviceID: 'Les anciens NZXT Kraken et l\'EVGA CLC ont le même ID de périphérique et CoolerControl ne peut pas déterminer quel appareil est connecté. Cela est nécessaire pour une bonne communication avec l\'appareil.',
      restartRequired: 'Un redémarrage des services systemd de CoolerControl peut être nécessaire et sera géré automatiquement si besoin.',
      deviceModel: 'Le périphérique Liquidctl est-il l\'un des modèles suivants ?',
      modelList: 'NZXT Kraken X40, X60, X31, X41, X51 ou X61',
      acceptLabel: "Oui, c'est un appareil Kraken ancien",
      rejectLabel: "Non, c'est un appareil EVGA CLC"
    },
    password: {
      title: 'Entrez Votre Mot de Passe',
      newPasswordTitle: 'Entrez Un Nouveau Mot de Passe',
      invalidPassword: 'Mot de Passe Invalide',
      passwordHelp: 'Lors de l\'installation, le daemon utilise un mot de passe par défaut pour protéger les points de contrôle des appareils. \nVous pouvez éventuellement créer un mot de passe fort pour une meilleure protection. \nSi vous voyez cette boîte de dialogue et que vous n\'avez pas encore défini de mot de passe, essayez d\'actualiser l\'interface utilisateur \n ou cliquez sur Connexion dans le menu Protection d\'Accès. Consultez le wiki du projet pour plus d\'informations.'
    },
    notFound: {
      message: 'Tout comme la distribution Linux 🐧 parfaite,\ncette page n\'existe pas.'
    },
    helloWorld: {
      message: "Vous avez créé avec succès un projet avec Vite + Vue 3. Quelle est la suite ?"
    },
    dashboardInfo: {
      description: 'Les tableaux de bord vous permettent de visualiser les données des capteurs de votre système selon vos préférences. Vous pouvez choisir entre des graphiques temporels ou tabulaires et ajuster les filtres et les paramètres de chaque graphique pour vous concentrer sur les données spécifiques que vous souhaitez voir. De plus, vous pouvez créer plusieurs tableaux de bord personnalisés pour répondre à vos besoins.'
    },
    modeInfo: {
      description: 'Les modes vous permettent de sauvegarder les paramètres des canaux des appareils pour une application rapide et facile. Par exemple, vous pouvez créer un mode "Jeu" et un mode "Silencieux", vous permettant de basculer facilement entre eux.',
      note: 'Veuillez noter que la création de différents profils de ventilateur peut être nécessaire pour chaque mode, car les modes n\'incluent que les configurations de canal et n\'englobent pas les paramètres internes de profil ou de fonction.'
    },
    alertInfo: {
      description: 'Les alertes sont utilisées pour vous avertir lorsque des conditions spécifiques se produisent. Elles peuvent surveiller les températures et les vitesses des ventilateurs pour s\'assurer que votre système fonctionne correctement. Les alertes sont configurées pour des plages de valeurs de capteur spécifiques et envoient des notifications lorsque les valeurs dépassent ou reviennent dans des plages de seuil acceptables.'
    },
    customSensorInfo: {
      title: 'Aperçu des Capteurs Personnalisés',
      description: 'Les capteurs personnalisés vous permettent de combiner des capteurs existants de différentes manières, améliorant votre contrôle et votre efficacité sur le refroidissement du système. De plus, ils prennent en charge les données basées sur des fichiers, vous permettant de scripter des entrées de capteurs externes pour plus de flexibilité.',
      note: 'Remarque : Vous pouvez utiliser des profils de mélange pour combiner plusieurs sorties de capteurs personnalisés.'
    },
    functionInfo: {
      title: 'Aperçu des Fonctions',
      description: 'Les fonctions sont des algorithmes configurables appliqués aux sorties de profil. Elles vous permettent de gérer quand les changements de vitesse des ventilateurs se produisent, d\'ajuster les paramètres d\'hystérésis et d\'utiliser des moyennes mobiles pour les températures dynamiques.',
      identityFunction: 'La fonction Identité est l\'option la plus simple car elle ne modifie pas la sortie calculée du profil ; elle vous permet seulement de définir des plages minimales et maximales de changement de vitesse. Cela est particulièrement bénéfique pour minimiser les fluctuations constantes de vitesse des ventilateurs.'
    },
    profileInfo: {
      title: 'Aperçu des Profils',
      description: 'Les profils définissent des paramètres personnalisables pour contrôler les vitesses des ventilateurs, le même profil pouvant être utilisé pour plusieurs ventilateurs. Les types incluent :',
      type: {
        fixed: 'Vitesse Fixe',
        fanCurve: 'Courbe de Ventilateur/Graphique',
        mix: 'Profil de Mélange',
        default: 'Paramètres par Défaut de l\'Appareil'
      },
      additionalInfo: 'Les profils sont la base pour contrôler les vitesses des ventilateurs et peuvent être améliorés davantage en appliquant des fonctions d\'algorithme plus avancées.'
    },
    deviceInfo: {
      details: 'Détails de l\'Appareil',
      systemName: 'Nom du Système',
      deviceType: 'Type d\'Appareil',
      deviceUID: 'UID de l\'Appareil',
      firmwareVersion: 'Version du Firmware',
      model: 'Modèle',
      driverName: 'Nom du Pilote',
      driverType: 'Type de Pilote',
      driverVersion: 'Version du Pilote',
      locations: 'Emplacements'
    },
    onboarding: {
      welcome: 'Bienvenue dans CoolerControl !',
      beforeStart: 'Avant de commencer, l\'une des choses les plus importantes à savoir est',
      settingUpDrivers: 'la configuration de vos pilotes matériels',
      fansNotShowing: 'Si vos ventilateurs n\'apparaissent pas ou ne peuvent pas être contrôlés, il y a probablement un problème avec les pilotes du noyau actuellement installés.',
      checkDocs: 'Avant d\'ouvrir un problème, veuillez confirmer que tous les pilotes ont été correctement chargés en',
      checkingDocs: 'consultant la documentation de Support Matériel',
      startTourAgain: 'Remarque : vous pouvez recommencer cette visite à tout moment depuis la page des paramètres.',
      letsStart: 'D\'accord, commençons !',
      dashboards: 'Tableaux de Bord',
      dashboardsDesc: 'Les tableaux de bord sont une collection organisée de graphiques pour visualiser les données des capteurs de votre système.',
      profiles: 'Profils',
      profilesDesc: 'Les profils définissent des paramètres personnalisables pour contrôler les vitesses des ventilateurs. Le même profil peut être utilisé pour plusieurs ventilateurs et appareils.',
      functions: 'Fonctions',
      functionsDesc: 'Les fonctions sont des algorithmes configurables qui peuvent être appliqués à la sortie d\'un profil. Cela peut être utile pour gérer quand les changements de vitesse des ventilateurs se produisent.',
      appInfo: 'Informations sur l\'Application et le Daemon',
      appInfoDesc: 'En cliquant sur le logo, vous ouvrez la page d\'Informations sur l\'Application, où vous pouvez obtenir des informations sur l\'application, le daemon du système et les journaux. C\'est là que vous devez aller lors du dépannage, et il y a un petit badge de statut du daemon ici pour vous informer de tout problème potentiel.',
      quickAdd: 'Ajout Rapide',
      quickAddDesc: 'Il s\'agit d\'un menu pour ajouter facilement de nouveaux éléments comme des Tableaux de bord, des Profils, etc.',
      dashboardQuick: 'Menu Rapide du Tableau de Bord',
      dashboardQuickDesc: 'Il s\'agit d\'un menu pour accéder rapidement à vos tableaux de bord, même si le menu principal est réduit.',
      settings: 'Paramètres',
      settingsDesc: 'Ce bouton ouvrira la page des paramètres contenant différents paramètres d\'interface utilisateur et de daemon.',
      restartMenu: 'Menu de Redémarrage',
      restartMenuDesc: 'Ici, vous pouvez choisir de recharger l\'interface utilisateur ou de redémarrer le daemon du système.',
      thatsIt: 'C\'est tout !',
      ready: 'Et n\'oubliez pas, si vos ventilateurs n\'apparaissent pas ou ne peuvent pas être contrôlés, consultez la documentation de Support Matériel',
      startNow: 'D\'accord, vous êtes prêt à commencer !'
    },
    axisOptions: {
      title: 'Options d\'Axe',
      autoScale: 'AutoÉchelle',
      max: 'Max',
      min: 'Min',
      dutyTemperature: 'Cycle / Température',
      rpmMhz: 'tr/min / MHz',
      krpmGhz: 'k tr/min / GHz',
      watts: 'watts'
    },
    sensorTable: {
      device: 'Appareil',
      channel: 'Canal',
      current: 'Actuel',
      min: 'Min',
      max: 'Max',
      average: 'Moyenne'
    },
    modeTable: {
      setting: 'Paramètre'
    }
  },
  auth: {
    enterPassword: 'Entrez Votre Mot de Passe',
    setNewPassword: 'Entrez Un Nouveau Mot de Passe',
    loginFailed: 'Échec de Connexion',
    invalidPassword: 'Mot de Passe Invalide',
    passwordSetFailed: 'Échec de Définition du Mot de Passe',
    passwordSetSuccessfully: 'Nouveau mot de passe défini avec succès',
    logoutSuccessful: 'Vous vous êtes déconnecté avec succès.',
    unauthorizedAction: 'Vous devez être connecté pour effectuer cette action'
  },
  device: {
    processInterrupted: ' - Processus interrompu.',
    modelSetSuccessfully: 'Type de modèle d\'appareil défini avec succès.',
    modelSetRestartInProgress: 'Type de modèle d\'appareil défini avec succès. Redémarrage en cours.'
  },
  daemon: {
    status: {
      ok: 'Ok',
      hasWarnings: 'A des Avertissements',
      hasErrors: 'A des Erreurs',
      haswarnings: 'A des Avertissements',
      haserrors: 'A des Erreurs'
    }
  },
  device_store: {
    unauthorized: {
      summary: 'Non Autorisé',
      detail: 'Vous devez être connecté pour effectuer cette action'
    },
    login: {
      success: {
        summary: 'Succès',
        detail: 'Connexion réussie.'
      },
      failed: {
        summary: 'Échec de Connexion',
        detail: 'Mot de Passe Invalide'
      }
    },
    logout: {
      summary: 'Déconnexion',
      detail: 'Vous vous êtes déconnecté avec succès.'
    },
    password: {
      set_success: {
        summary: 'Mot de Passe',
        detail: 'Nouveau mot de passe défini avec succès'
      },
      set_failed: {
        summary: 'Échec de Définition du Mot de Passe'
      }
    },
    asetek: {
      header: 'Appareil Inconnu Détecté',
      success: {
        summary: 'Succès',
        detail_legacy: 'Type de modèle d\'appareil défini avec succès. Redémarrage en cours.',
        detail_evga: 'Type de modèle d\'appareil défini avec succès.'
      },
      error: {
        summary: 'Erreur',
        detail: 'Processus interrompu.'
      }
    }
  },
  models: {
    chartType: {
      timeChart: 'Graphique Temporel',
      table: 'Tableau',
      controls: 'Contrôles'
    },
    dataType: {
      temp: 'Temp',
      duty: 'Cycle',
      load: 'Charge',
      rpm: 'tr/min',
      freq: 'Fréq',
      watts: 'Watts'
    },
    profile: {
      profileType: {
        default: 'Par Défaut',
        fixed: 'Fixe',
        graph: 'Graphique',
        mix: 'Mélange'
      },
      functionType: {
        identity: 'Identité',
        standard: 'Standard',
        exponentialMovingAvg: 'Moyenne Mobile Exponentielle'
      },
      mixFunctionType: {
        min: 'Minimum',
        max: 'Maximum',
        avg: 'Moyenne'
      }
    },
    customSensor: {
      sensorType: {
        mix: 'Mélange',
        file: 'Fichier'
      },
      mixFunctionType: {
        min: 'Minimum',
        max: 'Maximum',
        delta: 'Delta',
        avg: 'Moyenne',
        weightedAvg: 'Moyenne Pondérée'
      }
    },
    themeMode: {
      system: 'Système',
      dark: 'Sombre',
      light: 'Clair',
      highContrastDark: 'Sombre à Haut Contraste',
      highContrastLight: 'Clair à Haut Contraste',
      custom: 'Thème Personnalisé'
    },
    channelViewType: {
      control: 'Contrôle',
      dashboard: 'Tableau de Bord'
    },
    alertState: {
      active: 'Actif',
      inactive: 'Inactif'
    },
    deviceType: {
      customSensors: 'Capteurs Personnalisés',
      cpu: 'CPU',
      gpu: 'GPU',
      liquidctl: 'Liquidctl',
      hwmon: 'Hwmon'
    },
    driverType: {
      kernel: 'Noyau',
      liquidctl: 'Liquidctl',
      nvml: 'NVML',
      nvidiaCli: 'Nvidia CLI',
      coolercontrol: 'CoolerControl'
    },
    lcdModeType: {
      none: 'Aucun',
      liquidctl: 'Liquidctl',
      custom: 'Personnalisé'
    }
  }
} 
// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

export default {
    common: {
        save: 'Guardar',
        mouseActions: 'Acciones del ratón',
        moreInfo: 'Más información',
        cancel: 'Cancelar',
        add: 'Añadir',
        yes: 'Sí',
        no: 'No',
        ok: 'Aceptar',
        error: 'Error',
        success: 'Éxito',
        loading: 'Cargando...',
        restarting: 'Reiniciando...',
        retry: 'Reintentar',
        saveAndRefresh: 'Guardar y actualizar',
        reset: 'Restablecer',
        sslTls: 'SSL/TLS',
        protocol: 'Protocolo',
        address: 'Dirección',
        port: 'Puerto',
        search: 'Buscar',
        finish: 'Finalizar',
        next: 'Siguiente',
        previous: 'Anterior',
        unmanaged: 'Sin gestión',
        password: 'Contraseña',
        currentPassword: 'Contraseña actual',
        newPassword: 'Nueva contraseña',
        confirmPassword: 'Confirmar contraseña',
        savePassword: 'Guardar contraseña',
        state: 'Estado',
        name: 'Nombre',
        message: 'Mensaje',
        timestamp: 'Marca de tiempo',
        temperature: 'Temp.',
        duty: 'Velocidad',
        offset: 'Desplazamiento',
        stay: 'Quedarse',
        discard: 'Descartar',
        copy: '(copia)',
        minuteAbbr: 'min',
        rpmAbbr: 'rpm',
        mhzAbbr: 'MHz',
        ghzAbbr: 'GHz',
        tempUnit: '°C',
        percentUnit: '%',
        secondAbbr: 's',
        wattAbbr: 'W',
        toast: {
            modeCreated: 'Modo Creado',
            modeDuplicated: 'Modo Duplicado',
            modeNameUpdated: 'Nombre del Modo Actualizado',
            modeUpdated: 'Modo actualizado con la configuración actual',
            modeDeleted: 'Modo Eliminado',
            modeActivated: 'Modo Activado',
            customSensorSaved: 'Sensor Personalizado Guardado y Actualizando UI...',
            customSensorUpdated:
                'Sensor Personalizado actualizado correctamente y Actualizando UI...',
            customSensorDeleted:
                'Sensor Personalizado eliminado correctamente y Actualizando UI...',
            alertSaved: 'Alerta Guardada',
            alertUpdated: 'Alerta Actualizada',
            alertDeleted: 'Alerta Eliminada',
            alertNotFound: 'No se encontró la Alerta para actualizar',
            settingsUpdated: 'Configuración actualizada correctamente y aplicada al dispositivo',
            settingsError: 'Hubo un error al intentar aplicar esta configuración',
            thinkPadFanControlApplied: 'Control de Ventilador ThinkPad aplicado correctamente',
        },
    },
    layout: {
        shell: {
            search: {
                hint: 'Buscar dispositivos, sensores, ajustes y acciones',
                recent: 'Reciente',
                jumpTo: 'Ir a',
                noResults: 'No se encontraron coincidencias.',
                more: '{count} más',
                kindFan: 'Ventiladores',
                kindSensor: 'Sensores',
                kindAction: 'Acciones',
                kindPage: 'Páginas',
            },
            home: 'Inicio',
            cooling: 'Refrigeración',
            monitoring: 'Monitoreo',
            devices: 'Dispositivos',
            settings: 'Configuración',
            plugins: 'Plugins',
            modes: 'Modos',
            manageModes: 'Gestionar modos',
            access: 'Acceso',
            power: 'Energía',
            noModes: 'No hay modos guardados',
            supportWizards: {
                summary: '¡Magos del soporte activados!',
                detail: 'Gracias a los voluntarios que ayudan a nuestros usuarios a hacer funcionar su hardware y sus controladores.',
            },
            coolingPanel: {
                pinned: 'Fijado',
                pin: 'Fijar',
                unpin: 'Desfijar',
                library: 'Perfiles y Funciones',
                profiles: 'Perfiles',
                functions: 'Funciones',
                addFolder: 'Añadir carpeta',
                newFolder: 'Nueva carpeta',
                deleteFolder: 'Eliminar carpeta',
            },
            monitoringPanel: {
                newDashboard: 'Nuevo Panel',
                createAlert: 'Crear alerta para este sensor',
                failAlert: 'Crear una alerta de fallo (se activa a 0 rpm)',
                failAlertSuffix: 'Fallo',
            },
            devicesPanel: {
                disabled: 'Deshabilitado',
            },
            sensorDest: {
                monitoring: 'Monitoreo',
                cooling: 'Refrigeración',
                lighting: 'Iluminación',
                lcd: 'LCD',
            },
            manageSensors: {
                title: 'Gestionar Dispositivos y Sensores',
                hint: 'Habilite o deshabilite dispositivos y sensores. Se recomienda deshabilitar los que no se utilizan.',
                pendingChanges: 'Sin cambios | {count} cambio | {count} cambios',
                applyRestart: 'Aplicar y Reiniciar',
                disabledDevices: 'Dispositivos Deshabilitados',
                openButton: 'Gestionar Dispositivos y Sensores',
            },
            toast: {
                copy: 'Copiar',
                dismissAll: 'Descartar todo',
            },
            homePanel: {
                overview: 'Resumen',
                logs: 'Registros',
            },
            homePage: {
                viewLogs: 'Ver Registros',
                logsAll: 'Todos',
                logsWarnings: 'Advertencias+',
                logsErrors: 'Errores',
                logsNoMatches: 'No hay líneas de registro coincidentes.',
                getStartedGroup: 'Primeros Pasos',
                learnGroup: 'Aprender',
                resourcesGroup: 'Recursos',
                modeAndAlerts: 'Modo y Alertas',
                noActiveMode: 'Sin modo activo',
                setUpCooling: 'Configurar refrigeración',
            },
            devicesPage: {
                landingHint: 'Seleccione un dispositivo para ver sus detalles y configuración.',
                temps: 'temps',
                fans: 'ventiladores',
                lighting: 'iluminación',
                lcd: 'LCD',
                deviceDisabled: 'Este dispositivo está deshabilitado.',
                enableDevice: 'Habilitar Dispositivo',
                disableUnusedSensors: 'Deshabilitar sensores no utilizados… (recomendado)',
                sensors: 'Sensores',
            },
            hardwareHelp: {
                missingDevice: '¿Esperabas hardware que no aparece aquí?',
            },
            coolingPage: {
                landingHint:
                    'Seleccione un ventilador o bomba para ver y ajustar su refrigeración.',
                noChannels: 'No se detectaron canales de ventilador o bomba.',
                noneControllable:
                    'Ninguno de los canales de ventilador o bomba detectados se puede controlar.',
                noticeBlockedByEnvironment:
                    'La detección de hardware no pudo ejecutarse, por lo que pueden faltar canales de ventilador y bomba.',
                fullChart: 'Gráfico completo',
                guidedSetup: 'Configuración Guiada',
                setupMenu: {
                    autoCreateThisFan: 'Crear automáticamente para este ventilador',
                    createProfile: 'Crear un perfil nuevo',
                    calibrateThisFan: 'Calibrar este ventilador',
                    autoCreateAllFans: 'Crear automáticamente para todos los ventiladores',
                    calibrateAllFans: 'Calibrar todos los ventiladores',
                },
                manualAt: 'Manual {duty}%',
                manualDuty: 'Ciclo manual',
                modeProfile: 'Perfil',
                modeManual: 'Manual',
                modeUnmanaged: 'Sin gestión',
                unmanagedHint:
                    'El dispositivo o su firmware controla este canal. CoolerControl no enviará ningún comando de velocidad.',
                apply: 'Aplicar',
                saveAndApply: 'Guardar y Aplicar',
                unsavedChanges: 'Hay cambios en este canal que no se han aplicado.',
                unsavedChangesHeader: 'Cambios sin guardar',
                selectProfile: 'Seleccionar un perfil',
                sharedWith: 'Compartido con {count} más',
                sharedTooltip: 'Este perfil también controla otros canales.',
                notShared: 'Solo este ventilador',
                notSharedTooltip: 'Este perfil solo controla este canal.',
                forkForFan: 'Bifurcar para este ventilador',
                forkQualifier: 'copia de {channel}',
                fork: {
                    confirmHeader: 'Bifurcar para este ventilador',
                    confirmMessage:
                        "Copiar el perfil '{profile}' a un nuevo perfil '{copy}' y asignarlo a {channel}.\n\nEl original queda intacto, así que los cambios aquí solo afectarán a {channel}.",
                    accept: 'Crear copia',
                },
                convert: {
                    button: 'Convertir para la calibración',
                    tooltip:
                        'Este ventilador está calibrado, así que sus velocidades guardadas ahora se leen como velocidades reales y se reasignan en cada escritura. Conviértelas para que el ventilador se comporte como antes de la calibración.',
                    confirmHeader: 'Convertir para el ventilador calibrado',
                    confirmProfile:
                        "Copiar el perfil '{profile}' a un nuevo perfil '{copy}', convertir sus velocidades y asignarlo a {channel}.\n\nConvierte solo velocidades que definiste antes de calibrar este ventilador. Convertir dos veces hace que el ventilador gire a una velocidad incorrecta. El original queda intacto.",
                    confirmManual:
                        'Convertir el ciclo manual de {channel} para que el ventilador mantenga la velocidad que tenía antes de la calibración.\n\nConvierte solo un valor que definiste antes de calibrar este ventilador. Convertirlo dos veces hace que el ventilador gire a una velocidad incorrecta.',
                    nameQualifier: 'calibrado',
                    accept: 'Convertir',
                    successProfile:
                        "Se asignó '{profile}' a {channel} con las velocidades convertidas.",
                    successManual: 'Ciclo manual convertido a {duty}%.',
                    error: 'No se pudieron convertir las velocidades de este ventilador.',
                    floorHeading: 'Algunos puntos se convirtieron a 0%',
                    floorNotice:
                        '{count} punto(s) estaban por debajo de la velocidad mínima que se puede fijar en {channel} tras la calibración, así que se convirtieron a 0%. Revisa la nueva curva antes de confiar en ella.',
                    modesHeading: 'Los modos siguen usando el original',
                    modesReminder:
                        'Estos modos siguen asignando el perfil original a {channel}: {modes}. Actualízalos para usar la copia convertida.',
                },
                notControllable:
                    'Este canal informa su velocidad pero CoolerControl no puede controlarlo.',
                verdictFirmwareOverride:
                    'CoolerControl estableció este canal en control manual, pero el firmware lo revirtió.',
                verdictFamilyMayNeedOutOfTree:
                    'No se encontró un control de ventilador escribible para este canal. En esta familia de chips, a veces lo proporciona otro controlador del kernel.',
                verdictNotSupportedByDriver:
                    'El controlador en uso no expone control de ventilador para este canal.',
                verdictNoPwm:
                    'El controlador cargado no expone control de ventilador para este canal, solo su velocidad.',
                verdictPwmReadOnly:
                    'El controlador cargado expone un control de ventilador para este canal, pero lo marca como de solo lectura.',
                verdictIgnoresDuty:
                    'Este canal aceptó cambios de potencia, pero su velocidad medida nunca respondió.',
                verdictUnverifiable:
                    'Este canal no tiene un tacómetro utilizable, por lo que no se puede verificar su respuesta a los cambios de potencia.',
                verdictEvidenceLabel: 'Medido en esta máquina:',
                evidenceNoPwmFile: 'sin control de ventilador expuesto',
                evidencePwmNotWritable: 'el control de ventilador es de solo lectura',
                evidenceHasTachometer: 'lectura de velocidad disponible',
                evidenceNoTachometer: 'sin lectura de velocidad',
                verdictLearnMore: '¿Qué puedo hacer al respecto?',
                verdictFoundSomethingThatWorks: '¿Encontró algo que funciona? Cuéntenoslo',
                activeMode: 'Activo',
                previousMode: 'Anterior',
                activate: 'Activar',
                noModes:
                    'Aún no hay modos guardados. Los modos capturan todas las configuraciones de canal para un cambio rápido.',
                powerProfiles: {
                    title: 'Perfil de energia del sistema',
                    description:
                        'Activar un modo automaticamente cuando cambie el perfil de energia del sistema.',
                    activeProfile: 'Perfil actual: {profile}',
                    noMode: 'Ningun modo',
                    saveFailed: 'No se pudo guardar la asignacion de perfiles de energia.',
                    profileNames: {
                        'power-saver': 'Ahorro de energia',
                        balanced: 'Equilibrado',
                        performance: 'Rendimiento',
                    },
                },
                miniCurveHint:
                    'Curva del perfil asignado. El punto marca el objetivo a la temperatura actual de la fuente; la Función del canal da forma al valor real.',
                chain: {
                    tempSource: 'Fuente de temperatura',
                    profile: 'Perfil',
                    function: 'Función',
                },
            },
        },
        topbar: {
            login: 'Iniciar sesión',
            logout: 'Cerrar sesión',
            changePassword: 'Cambiar contraseña',
            accessTokens: 'Tokens de acceso',
            restartUI: 'Reiniciar interfaz',
            restartDaemonAndUI: 'Reiniciar daemon e interfaz',
            restartConfirmMessage: '¿Está seguro de que desea reiniciar el daemon y la interfaz?',
            restartConfirmHeader: 'Reinicio del daemon',
            shutdownSuccess: 'Señal de apagado del daemon aceptada',
            shutdownError:
                'Error desconocido al enviar la señal de apagado. Consulte los registros para más detalles.',
            quitDesktopApp: 'Salir de la aplicación',
            back: 'Atrás',
            expandMenu: 'Expandir menú',
            collapseMenu: 'Contraer menú',
            alerts: 'Alertas',
            settings: 'Configuración',
            openInBrowser: 'Abrir en navegador',
            loginSuccessful: 'Inicio de sesión exitoso',
        },
        settings: {
            title: 'Configuración',
            devices: {
                toggleRequiresRestart:
                    'Cambiar dispositivos o sensores requiere un reinicio del daemon y la interfaz. ¿Está seguro de que desea hacer esto ahora?',
                enableDevices: 'Habilitar dispositivos',
                unknownError:
                    'Error desconocido al intentar aplicar cambios a todos los dispositivos. Consulte los registros para más detalles.',
            },
            plugins: {
                privileged: 'Acceso privilegiado',
                pluginUrl: 'Página de inicio',
                restricted: 'Acceso restringido',
                settingsSaved: 'Configuración del plugin guardada correctamente',
                settingsNotSaved: 'Error al guardar la configuración del plugin',
            },
            appearance: 'Apariencia',
            general: 'General',
            language: 'Idioma',
            selectLanguage: 'Seleccionar idioma',
            systemLanguage: 'Sistema',
            fullScreen: 'Pantalla completa',
            railToCollapse: 'Barra de navegación para contraer',
            eyeCandy: 'Efectos visuales',
            interfaceFont: 'Fuente de la interfaz',
            introduction: 'Introducción',
            startTour: 'Iniciar tour',
            timeFormat: 'Formato de hora',
            time24h: '24 horas',
            time12h: '12 horas',
            frequencyPrecision: 'Precisión de frecuencia',
            startupPage: 'Página de inicio',
            dashboardLineSize: 'Tamaño de línea del panel',
            themeStyle: 'Estilo del tema',
            themeGroups: {
                builtIn: 'Integrados',
                installed: 'Instalados',
                custom: 'Personalizado',
            },
            desktop: 'Escritorio',
            startInTray: 'Iniciar en bandeja',
            closeToTray: 'Cerrar a bandeja',
            zoom: 'Zoom',
            desktopStartupDelay: 'Retraso de inicio de escritorio',
            groups: {
                startup: 'Inicio',
                performance: 'Rendimiento',
                devices: 'Dispositivos y detección',
                liquidctl: 'Liquidctl',
            },
            applySettingsOnStartup: 'Aplicar configuración al inicio',
            deviceDelayAtStartup: 'Retraso de dispositivo al inicio',
            pollingRate: 'Tasa de sondeo',
            compressApiPayload: 'Comprimir carga útil de API',
            liquidctlIntegration: 'Integración con Liquidctl',
            liquidctlDeviceInit: 'Inicialización de dispositivos Liquidctl',
            hideDuplicateDevices: 'Ocultar dispositivos duplicados',
            drivePowerState: 'Estado de energía del disco',
            sensorsAutoDetect: 'Detección automática de sensores',
            sensorsConfig: 'Configuración de lm-sensors',
            deviceListener: 'Listener de cambios de dispositivos',
            customTheme: {
                title: 'Tema Personalizado',
                accent: 'Color de Acento',
                accentGradientTo: 'Fin del degradado de acento',
                bgOne: 'Fondo Primario',
                bgTwo: 'Fondo Secundario',
                border: 'Color del Borde',
                text: 'Color del Texto',
                textSecondary: 'Color del Texto Secundario',
                success: 'Éxito',
                warning: 'Advertencia',
                error: 'Error',
                info: 'Información',
                export: 'Exportar Tema',
                import: 'Importar Tema',
                copyCode: 'Copiar Código',
                pasteCode: 'Pegar Código',
                themeCodeCopied: 'Código del tema copiado',
                themeApplied: 'Tema aplicado',
                invalidThemeCode: 'Código de tema no válido',
            },
            tooltips: {
                timeFormat: 'Formato de hora: 12 horas (AM/PM) o 24 horas',
                frequencyPrecision: 'Ajustar la precisión de los valores de frecuencia mostrados.',
                startupPage: 'La página que se muestra después de cargar la aplicación.',
                railToCollapse:
                    'Usar también el área vacía de la barra de navegación para expandir o contraer el menú.',
                eyeCandy:
                    'Habilitar animaciones visuales como iconos de ventiladores giratorios.\nEsto utilizará algunos recursos adicionales de la GPU.',
                interfaceFont:
                    'Usar las fuentes incluidas con CoolerControl o las configuradas en su sistema.',
                fullScreen: 'Alternar el modo de pantalla completa',
                lineThickness: 'Ajustar el grosor de las líneas de los gráficos en el panel',
                startInTray:
                    'Al iniciar, la ventana principal de la interfaz de usuario estará oculta y solo\nserá visible el icono de la bandeja del sistema.',
                closeToTray:
                    'Cerrar la ventana de la aplicación dejará la aplicación ejecutándose en la bandeja del sistema',
                zoom: 'Establecer manualmente el nivel de zoom de la interfaz de usuario.',
                desktopStartupDelay:
                    'Agrega un retraso antes de iniciar la aplicación de escritorio (en segundos).\nAyuda con problemas que surgen al tener la aplicación de escritorio\niniciada automáticamente al iniciar sesión o al iniciar demasiado rápido',
                unlockRange: 'Permitir valores fuera del rango recomendado',
                lockRange: 'Restringir al rango recomendado',
                applySettingsOnStartup:
                    'Aplicar automáticamente la configuración al iniciar el daemon y al despertar del sueño',
                deviceDelayAtStartup:
                    'Retraso antes de comenzar la comunicación del dispositivo (en segundos).\nAyuda con dispositivos que tardan en inicializarse o se detectan de manera intermitente',
                pollingRate:
                    'La tasa a la que se sondean los datos del sensor (en segundos).\nUna tasa de sondeo más alta reducirá el uso de recursos, y una más baja aumentará la capacidad de respuesta.\nSe debe usar con precaución una tasa inferior a 1.0.',
                compressApiPayload: 'Habilitar la compresión de la carga útil de la API',
                liquidctlIntegration:
                    'Deshabilitar esto desactivará completamente la integración de Liquidctl,\nindependientemente del estado de instalación del paquete coolercontrol-liqctld. Si está disponible, se utilizarán controladores HWMon en su lugar.',
                liquidctlDeviceInit:
                    'Precaución: Deshabilite esto SOLO si usted, o otro programa,\nestán manejando la inicialización del dispositivo liquidctl.\nEsto puede ayudar a evitar conflictos con otros programas.',
                hideDuplicateDevices:
                    'Algunos dispositivos son compatibles tanto con los controladores Liquidctl como con los HWMon.\nLiquidctl se usa por defecto por sus características adicionales. Para usar controladores HWMon en su lugar,\ndeshabilite esto y el dispositivo liquidctl para evitar conflictos de controladores.',
                drivePowerState:
                    'Los SSD y los HDD en particular pueden detenerse y entrar en un estado de bajo consumo.\nEsta opción, cuando está habilitada y la unidad lo soporta, informará las temperaturas de la unidad\ncomo 0°C cuando esté detenida para que los perfiles del ventilador puedan ajustarse en consecuencia.',
                sensorsAutoDetect:
                    'Detectar automáticamente sensores de hardware Super-I/O y cargar\nmódulos del kernel al iniciar. (solo x86_64)',
                sensorsConfig:
                    'Usar los nombres de sensores y los sensores ocultos de los archivos\nde configuración de lm-sensors (/etc/sensors3.conf y /etc/sensors.d).\nLos nombres definidos en CoolerControl siempre tienen prioridad.',
                deviceListener:
                    'Escuchar eventos de adición/eliminación de dispositivos (ej. conexión USB)\ny notificar cuando se detecten cambios de hardware.',
                triggersDaemonRestart: 'Activa un reinicio automático del daemon',
                copyThemeCode:
                    'Copia un código compacto que representa tu tema personalizado actual.\nCompártelo en chats o foros.',
                pasteThemeCode:
                    'Aplica un tema personalizado desde un código (cct1:...) que te hayan compartido.',
            },
            applySettingAndRestart:
                'Cambiar esta configuración requiere un reinicio del daemon y la interfaz de usuario. ¿Está seguro de que desea hacer esto ahora?',
            restartHeader: 'Aplicar configuración y reiniciar',
            success: 'Éxito',
            successDetail: 'Operación completada con éxito',
            languageChangeConfirm: '¿Cambiar idioma?',
            languageChangeConfirmMessage:
                '¿Está seguro de que desea continuar? Si algunos elementos de la interfaz no se muestran correctamente, actualice la página manualmente.',
            languageChangeSuccess: 'Idioma cambiado con éxito.',
            languageChangeError: 'Error al cambiar el idioma. Por favor, inténtelo de nuevo.',
            themeChangeSuccess: 'Tema cambiado con éxito.',
        },
        menu: {
            dashboards: 'Paneles',
            customSensors: 'Sensores personalizados',
            alerts: 'Alertas',
            pinned: 'Fijado',
            tooltips: {
                createMode: 'Crear modo desde configuración actual',
                addProfile: 'Añadir perfil',
                addAlert: 'Añadir alerta',
                addDashboard: 'Añadir panel',
                duplicate: 'Duplicar',
                rename: 'Renombrar',
                addCustomSensor: 'Añadir sensor personalizado',
                addFunction: 'Añadir función',
                chooseColor: 'Elegir color',
            },
        },
        plugins: {
            plugins: 'Plugins',
            notFound: 'Plugin no encontrado',
            type: 'Tipo',
            address: 'Dirección',
            privileges: 'Privilegios',
            url: 'URL',
            start: 'Iniciar',
            stop: 'Detener',
            restart: 'Reiniciar',
            started: 'Plugin iniciado',
            stopped: 'Plugin detenido',
            restarted: 'Plugin reiniciado',
            startFailed: 'Error al iniciar el plugin',
            stopFailed: 'Error al detener el plugin',
            restartFailed: 'Error al reiniciar el plugin',
            overview: 'Vista General de Plugins',
            gettingStarted:
                'Los plugins amplían CoolerControl con soporte adicional de dispositivos, integraciones y automatización. Pueden proporcionar nuevos sensores y controles de dispositivos, conectarse a servicios externos o agregar páginas de interfaz personalizadas.',
            findPlugins: 'Buscar e instalar Plugins',
            restartNote:
                'Si recientemente agregó un nuevo plugin y no aparece aquí, reinicie el demonio de CoolerControl.',
            containerNote:
                'Al ejecutar CoolerControl en un contenedor, los plugins deben colocarse en la carpeta compartida virtual persistente para que sobrevivan a los reinicios del contenedor.',
            installedPlugins: 'Plugins Instalados',
            noPlugins: 'No hay plugins instalados',
            info: 'Info',
            description: 'Descripción',
            enable: 'Habilitar',
            disable: 'Deshabilitar',
            pluginDisabled: 'Plugin deshabilitado.',
            pluginEnabled: 'Plugin habilitado.',
            pluginDisabledRestart: 'Plugin deshabilitado. Reinicie el daemon para aplicar.',
            pluginEnabledRestart: 'Plugin habilitado. Reinicie el daemon para aplicar.',
            disableFailed: 'Error al deshabilitar el plugin',
            enableFailed: 'Error al habilitar el plugin',
            serviceLogs: 'Registros del servicio',
            commandCopied: 'Comando copiado al portapapeles',
        },
        add: {
            profile: 'Perfil',
            function: 'Función',
            customSensor: 'Sensor personalizado',
        },
    },
    views: {
        daemon: {
            title: 'Daemon',
            daemonErrors: 'Errores del Daemon',
            daemonErrorsDetail:
                'El daemon ha reportado errores. Consulte los registros para más detalles.',
            daemonDisconnected: 'Daemon Desconectado',
            daemonDisconnectedDetail:
                'No se puede conectar al daemon. Por favor, compruebe si el daemon está en ejecución.',
            connectionRestored: 'Conexión Restaurada',
            connectionRestoredMessage: 'La conexión al daemon ha sido restaurada.',
            reconnecting: 'Reconectando...',
            disconnectedFor: 'Desconectado desde hace {time}',
        },
        speed: {
            applySetting: 'Aplicar Configuración',
        },
        customSensors: {
            missingSourcesNotice:
                'Las siguientes fuentes de temperatura ya no están presentes y se eliminarán al guardar: {sources}',
            sensorType: 'Tipo de Sensor',
            mixFunction: 'Función de Mezcla',
            howCalculateValue: 'Cómo calcular el valor resultante del sensor',
            tempFile: 'Archivo de Temperatura',
            filePathTooltip:
                'Introduzca la ruta absoluta al archivo de temperatura a utilizar para este sensor.\nEl archivo debe usar el formato de datos sysfs estándar:\nUn número de punto fijo en miligrados Celsius.\np.ej. 80000 para 80°C.\nEl archivo se verifica al enviarse.',
            browse: 'Explorar',
            browseCustomSensorFile: 'Explorar un archivo de sensor personalizado',
            tempSources: 'Fuentes de Temperatura',
            tempSource: 'Fuente de Temperatura',
            tempSourcesTooltip:
                'Fuentes de temperatura que se usarán en la función de mezcla<br/><i>Nota: Al combinar varios Sensores Personalizados solo se permiten relaciones directas de padre e hijo.<br/>Use Perfiles de Mezcla para configuraciones más complejas.</i>',
            offset: 'Desplazamiento',
            offsetTooltip:
                'Introduzca un desplazamiento negativo o positivo que se aplicará al sensor de origen.<br/><i>Nota: El valor final se limita a rangos normales de temperatura.</i>',
            timeWindow: 'Ventana de Suavizado',
            timeWindowTooltip:
                'Cuántos segundos de muestras recientes se suavizarán juntas.<br/><i>Nota: Debe estar entre 1 y 300 segundos.</i>',
            helpText: {
                mix: 'Combina múltiples fuentes de temperatura mediante la función elegida (Mín/Máx/Promedio/Delta/Promedio Ponderado). Use para controlar ventiladores desde el más caliente de varios sensores, o para equilibrar entre zonas.',
                file: 'Lee la temperatura desde una ruta de archivo. Use para sensores no detectados automáticamente por CoolerControl.',
                offset: 'Suma o resta un valor fijo a una fuente de temperatura. Use para calibrar una imprecisión conocida del sensor.',
                timeAverage:
                    'Media aritmética en una ventana de tiempo fija. La salida está acotada por el rango de entrada y nunca lo excede. Para ventiladores que deben ignorar picos breves de temperatura.',
                exponentialMovingAvg:
                    'Promedio ponderado que favorece las lecturas recientes. Más suave que el Promedio Temporal con la misma ventana, pero tarda aproximadamente 3 veces la longitud de la ventana en seguir completamente un cambio sostenido. Para ventiladores que deben seguir tendencias reales sin fluctuaciones.',
            },
            tempWeights: 'Pesos de Temperatura',
            tempName: 'Nombre de Temperatura',
            weight: 'Peso',
            saveCustomSensor: 'Guardar Sensor Personalizado',
            unsavedChanges: 'Hay cambios no guardados realizados en este Sensor Personalizado.',
            unsavedChangesHeader: 'Cambios no guardados',
            selectCustomSensorFile: 'Seleccionar Archivo de Sensor Personalizado',
            deleteCustomSensor: 'Eliminar Sensor Personalizado',
            deleteCustomSensorConfirm:
                '¿Está seguro de que desea eliminar el sensor personalizado: "{name}"?',
        },
        dashboard: {
            timeRange: 'Rango de Tiempo',
            chartType: 'Tipo de Gráfico',
            filterSensors: 'Filtrar Sensores',
            mouseActions:
                'Acciones del ratón en el panel:\n- Resaltar selección para hacer zoom.\n- Ctrl+Desplazar para hacer zoom.\n- Clic derecho para mover cuando se hace zoom.\n- Doble clic para restablecer y reanudar la actualización.\n- Ctrl+clic o clic medio para mostrar todos los sensores en la herramienta de ayuda.',
            fullPage: 'Página Completa',
            filterTags: 'Filtrar Etiquetas',
            filterByTag: 'Filtrar por Etiqueta',
            filterBySensor: 'Filtrar por Sensor',
            filterTypes: 'Filtrar Tipos',
            filterByDataType: 'Filtrar por Tipo de Datos',
            exitFullPage: 'Salir de Página Completa',
            deleteDashboard: 'Eliminar Panel',
            deleteDashboardConfirm: '¿Está seguro de que desea eliminar el panel: "{name}"?',
            setAsHome: 'Establecer como Inicio',
            duplicateDashboard: 'Duplicar Panel',
            openCooling: 'Abrir controles de refrigeración',
        },
        appInfo: {
            noWarranty: 'Este programa viene sin absolutamente ninguna garantía.',
            changeStartupPage: 'Cambiar la página de inicio en Ajustes',
            daemonStatus: 'Estado del Daemon',
            acknowledgeIssues: 'Reconocer Problemas',
            status: 'Estado',
            host: 'Host',
            uptime: 'Tiempo de funcionamiento',
            version: 'Versión',
            processId: 'ID del Proceso',
            memoryUsage: 'Uso de Memoria',
            liquidctl: 'Liquidctl',
            connected: 'Conectado',
            disconnected: 'Desconectado',
            helpfulLinks: 'Enlaces Útiles',
            uiTour: 'Recorrido por la UI',
            gettingStarted: 'Primeros Pasos',
            helpSettingUp: 'Ayuda para configurar el control de ventiladores',
            gettingStartedStep1: 'Abra Refrigeración y elija el ventilador que desea controlar.',
            gettingStartedStep2:
                'Elija Configuración Guiada y luego Nuevo Perfil para definir su curva del ventilador.',
            gettingStartedStep3: 'Reutilice ese Perfil en tantos ventiladores como desee.',
            gettingStartedAutoCreate:
                '{wizard} permite configurar perfiles básicos para todos sus ventiladores de una sola vez.',
            gettingStartedAutoCreateLink: 'Crear perfiles automáticamente',
            calibrateFansLink: 'calibre sus ventiladores',
            hardwareSupport: 'Soporte de Hardware',
            whatsNew: 'Novedades',
            logsAndDiagnostics: 'Registros y Diagnósticos',
            downloadCurrentLog: 'Descargar Registro Actual',
            deviceHealth: 'Estado de los Dispositivos',
            deviceHealthOk: 'Todos los sensores y canales funcionan correctamente.',
            detection: 'Detección de chips',
            detectionDescription:
                'Lo que encontró el sondeo del chip Super-I/O al iniciarse el demonio. Los módulos se cargan al arrancar, así que esta es la ejecución que explica un chip sin controlador.',
            detectionButton: 'Detección de chips',
            detectionNotRun:
                'No se ejecutó ninguna detección, por lo que no se sabe nada sobre los chips Super-I/O de esta máquina.',
            detectionSecureBoot: 'Arranque seguro',
            detectionContainer: 'Contenedor',
            detectionDevPort: '/dev/port disponible',
            detectionChips: 'Chips detectados',
            detectionNoChips: 'No se detectaron chips Super-I/O.',
            detectionBlacklisted: 'Controladores en lista negra',
            hardwareSupportOk: 'Todo el hardware detectado es compatible y controlable.',
            hardwareReport: 'Informe de hardware',
            hardwareReportDescription:
                'Un resumen de lo que CoolerControl ve en esta máquina, listo para pegar en un canal de soporte. Se excluyen números de serie e identificadores.',
            hardwareReportFull: 'Incluir el árbol hwmon completo',
            hardwareReportEmpty: 'No se pudo generar el informe.',
            hardwareReportButton: 'Informe de hardware',
            hardwareReportCopy: 'Copiar',
            hardwareReportCopied: 'Copiado',
            findingNoDriverBound: 'Se detectó un chip, pero ningún controlador cargado lo atiende.',
            findingBlacklisted: 'Este controlador está en la lista negra y no se cargó.',
            findingBlockedByEnvironment:
                'La detección de hardware no pudo ejecutarse en este entorno.',
            findingBlockedBySecureBoot:
                'La detección de hardware no pudo ejecutarse porque el Arranque seguro está activado.',
            findingBlockedByContainer:
                'La detección de hardware no pudo ejecutarse dentro de un contenedor.',
            findingBlockedByNoDevPort:
                'La detección de hardware no pudo ejecutarse porque /dev/port no está disponible.',
            findingDetectionUnsupported:
                'La detección de hardware no es compatible con esta arquitectura.',
            failsafeActive: 'Valores de seguridad en uso',
            deviceUnreachable: 'El dispositivo no responde',
            deviceUnreachableDetail:
                'El controlador dejó de responder, por lo que este dispositivo no se puede leer ni controlar. Se reintenta periódicamente.',
            missingTempSource: 'Fuente de temperatura faltante',
            staleTempSource: 'La fuente de temperatura usa valores de seguridad',
            stressTest: 'Pruebas de estrés térmico',
            stressTestTooltip:
                'Genera carga térmica sostenida para validar\ncurvas de ventilador y perfiles de enfriamiento.\nLos resultados pueden variar según el hardware.\nInstale stress-ng para backends adicionales.',
            cpuStress: 'Estrés de CPU',
            gpuStress: 'Estrés de GPU',
            gpuStressTooltip:
                'Puede requerir controladores Vulkan o OpenGL ES<br>al usar el backend integrado.',
            ramStress: 'Estrés de RAM',
            driveStress: 'Estrés de disco',
            driveStressTooltip:
                'Estrés de E/S en un dispositivo de bloque para generar<br>calor en los controladores de disco.<br>stress-ng requiere que el dispositivo esté montado.',
            builtInBackend: 'integrado',
            stressNgBackend: 'stress-ng',
            backendTooltip:
                'Elija el backend de la prueba de estrés.<br>El backend integrado funciona sin dependencias externas.<br>stress-ng (cuando está instalado) proporciona variantes adicionales de estresores.',
            selectDrive: 'Seleccionar disco',
            selectGpu: 'Seleccionar GPU',
            allGpus: 'Todas las GPU',
            start: 'Iniciar',
            stop: 'Detener',
            stopAll: 'Detener todo',
            active: 'Activo',
            inactive: 'Inactivo',
            psuWarningHeader: 'Advertencia de alto consumo',
            psuWarningMessage:
                'Ejecutar las pruebas de estrés de CPU y GPU simultáneamente generará una carga significativa en la fuente de alimentación. Si hace overclocking o usa una fuente de bajo vataje, puede producirse inestabilidad del sistema. ¿Desea continuar?',
            proceed: 'Continuar',
        },
        alerts: {
            triggersOutside: 'se activa por debajo de {min} o por encima de {max}{unit}',
            triggersAbove: 'se activa por encima de {max}{unit}',
            stateSince: '{state} desde {time}',
            deleteAlert: 'Eliminar Alerta',
            duplicateAlert: 'Duplicar Alerta',
            alertsOverview: 'Resumen de Alertas',
            alertLogs: 'Registros de Alertas',
            alertTriggered: 'Alerta Activada',
            alertRecovered: 'Alerta Recuperada',
            alertError: 'Error de alerta',
            alertSensorsReadable: 'Sensores de alerta legibles',
            deleteAlertConfirm: '¿Está seguro de que desea eliminar: "{name}"?',
            saveAlert: 'Guardar Alerta',
            channelSources: 'Fuentes de Canal para Alerta',
            channelSourcesTooltip:
                'Las fuentes de canal vigiladas por esta Alerta.\nUn tipo de sensor por Alerta: la primera selección filtra el resto.',
            triggerConditions: 'Condiciones de Activación',
            maxValueTooltip: 'Los valores por encima de esto activarán la alerta.',
            minValueTooltip: 'Los valores por debajo de esto activarán la alerta.',
            warmupDurationTooltip:
                'Cuánto tiempo debe estar activa una condición para que la alerta se considere activa.\nSe verifica solo a intervalos regulares de sondeo,\npor lo que puede no tener exactamente esta duración.',
            cooldownDurationTooltip:
                'Cuánto tiempo debe permanecer el valor dentro del rango antes de que la alerta se recupere.\nEvita alternancias rápidas entre activada y resuelta.',
            cooldownLessThan: 'condición recuperada por más tiempo que',
            repeatInterval: 'Repetir notificación cada',
            repeatIntervalTooltip:
                'Reenviar la notificación de escritorio a este intervalo mientras la alerta siga activa.\n0 desactiva las notificaciones repetidas.',
            enabled: 'Habilitada',
            enabledTooltip: 'Una alerta deshabilitada no se evalúa en absoluto.',
            sectionGeneral: 'General',
            sectionNotifications: 'Notificaciones',
            sectionActions: 'Acciones',
            silence: 'Silenciar',
            silenceTooltip:
                'Silenciar: suprime las notificaciones y el apagado durante un tiempo.\nLa alerta se sigue evaluando y muestra su estado.',
            silence15m: 'Silenciar durante 15 minutos',
            silence1h: 'Silenciar durante 1 hora',
            silence8h: 'Silenciar durante 8 horas',
            silence24h: 'Silenciar durante 24 horas',
            unsilence: 'Dejar de silenciar ahora',
            enableAlert: 'Habilitar Alerta',
            disableAlert: 'Deshabilitar Alerta',
            silencedUntil: 'Silenciada hasta {time}',
            disabledLabel: 'Deshabilitada',
            greaterThan: 'mayor que',
            lessThan: 'menor que',
            newAlert: 'Nueva Alerta',
            warmupGreaterThan: 'condición activada por más tiempo que',
            unsavedChanges: 'Hay cambios no guardados realizados en esta Alerta.',
            unsavedChangesHeader: 'Cambios no guardados',
            desktopNotify: 'notificación de escritorio',
            desktopNotifyTooltip:
                'Habilitar notificaciones de escritorio cuando se active la alerta.\n(Si es compatible)',
            desktopNotifyRecovery: 'notificación de escritorio en recuperación',
            desktopNotifyRecoveryTooltip:
                'Habilitar notificaciones de escritorio cuando la alerta se recupere.\n(Si es compatible)',
            desktopNotifyAudio: 'audio de notificación de escritorio',
            desktopNotifyAudioTooltip:
                'Habilitar audio de notificación de escritorio cuando se active la alerta.\n(Si es compatible)',
            shutdownOnActivation: 'apagar en activación',
            shutdownOnActivationTooltip:
                'Habilitar el apagado del sistema cuando se active la alerta.\nEl apagado del sistema comenzará un minuto después de que se active la alerta\ny se cancelará si la alerta se recupera.',
        },
        profiles: {
            targetDuty: 'Objetivo',
            actualDuty: 'Real',
            targetHint:
                'El objetivo se calcula a partir de las temperaturas actuales, antes de aplicar la Función del canal. El suavizado y la histéresis pueden hacer que el valor real difiera.',
            createProfile: 'Crear Perfil',
            deleteProfile: 'Eliminar Perfil',
            profileType: 'Tipo de Perfil',
            fixedDuty: 'Velocidad Fija del Ventilador',
            tempSource: 'Fuente de Temperatura',
            memberProfiles: 'Perfiles Miembros',
            mixFunction: 'Función de Mezcla',
            applyMixFunction: 'Aplicar función de mezcla a los perfiles seleccionados',
            profilesToMix: 'Perfiles para mezclar',
            saveProfile: 'Guardar Perfil',
            function: 'Función',
            functionToApply: 'Función a aplicar',
            graphProfileMouseActions:
                'Acciones del ratón en el Perfil Gráfico:\n- Ctrl+Desplazar para zoom.\n- Clic izquierdo en la línea para añadir punto.\n- Clic derecho en el punto para eliminar.\n- Arrastrar punto para mover.',
            unsavedChanges: 'Hay cambios no guardados en este Perfil.',
            unsavedChangesHeader: 'Cambios No Guardados',
            newProfile: 'Nuevo Perfil',
            tooltip: {
                profileType:
                    'Tipos de Perfiles:<br/>- Predeterminado: Sin gestión, devuelve el control al controlador del dispositivo<br/>- Fijo: Establece una velocidad constante<br/>- Gráfico: Curva de ventilador personalizable<br/>- Mezcla: Combina múltiples perfiles<br/>- Superposición: aplica un desplazamiento a la salida de un perfil existente',
            },
            profileDeleted: 'Perfil Eliminado',
            profileDuplicated: 'Perfil Duplicado',
            usedBy: 'Usado por',
            deleteProfileConfirm: '¿Está seguro de que desea eliminar: "{name}"?',
            deleteProfileWithChannelsConfirm:
                '"{name}" está siendo utilizado actualmente por: {channels}.\nEliminar este Perfil restablecerá la configuración de esos canales.\n¿Está seguro de que desea eliminar "{name}"?',
            profileUpdated: 'Perfil actualizado correctamente',
            profileUpdateError: 'Hubo un error al intentar actualizar este Perfil',
            tempSourceRequired: 'Se requiere una Fuente de Temperatura para un Perfil Gráfico.',
            memberProfilesRequired:
                'Se requieren al menos 2 Perfiles Miembros para un Perfil de Mezcla.',
            minProfileTemp: 'Temperatura Mínima del Perfil',
            maxProfileTemp: 'Temperatura Máxima del Perfil',
            staticOffset: 'Desplazamiento estático',
            offsetType: 'Tipo de desplazamiento',
            offsetTypeStatic: 'Desplazamiento estático',
            offsetTypeGraph: 'Desplazamiento de gráfico',
            baseProfile: 'Perfil base',
            baseProfileRequired: 'Se requiere un Perfil base para un Perfil de superposición.',
            profileOutputDuty: 'Velocidad de salida del perfil',
            offsetDuty: 'Velocidad de desplazamiento',
            points: 'Puntos',
            moveTable: 'Mover a otra esquina',
            addPointAfter: 'Añadir punto después',
            removePoint: 'Eliminar punto',
            curvePointLimitBadge: 'máx {n} pts',
            curveLimitedByAmdGpu:
                'Curva limitada a {n} puntos por la curva de ventilador del hardware AMD GPU.',
            curveLimitedByFirmware:
                'Curva limitada a {n} puntos por la curva de ventilador del firmware del dispositivo.',
        },
        modes: {
            createMode: 'Crear Modo',
            editMode: 'Editar Modo',
            updateToCurrent: 'Guardar la configuración actual en el modo',
            deleteMode: 'Eliminar Modo',
            deleteModeConfirm: '¿Está seguro de que desea eliminar el modo: "{name}"?',
            updateModeConfirm:
                '¿Está seguro de que desea sobrescribir "{name}" con la configuración actual?',
            duplicateMode: 'Duplicar Modo',
        },
        functions: {
            createFunction: 'Crear Función',
            deleteFunction: 'Eliminar Función',
            saveFunction: 'Guardar Función',
            stepSizeTitle: 'Tamaño de Paso',
            fixedStepSize: 'Fijo',
            fixedStepSizeTooltip:
                'Activado usa un tamaño de paso fijo para todos los cambios.\nDesactivado permite establecer un rango mínimo y máximo de tamaño de paso.',
            asymmetric: 'Asimétrico',
            asymmetricTooltip:
                'Cuando está activado, se pueden configurar límites de tamaño de paso separados para aumentos y disminuciones de velocidad.\nÚtil cuando desea que los ventiladores aceleren rápidamente pero desaceleren gradualmente, o viceversa.',
            stepSizeMin: 'Mínimo',
            stepSizeMinTooltip:
                'El cambio de velocidad del ventilador más pequeño que se aplicará.\nLos cambios menores se ignoran para reducir ajustes innecesarios.',
            stepSizeMax: 'Máximo',
            stepSizeMaxTooltip:
                'El cambio de velocidad del ventilador más grande permitido por actualización.\nLos cambios mayores se limitan a este valor para transiciones más suaves.',
            stepSizeFixed: 'Tamaño',
            stepSizeFixedTooltip:
                'Un único tamaño de paso aplicado a todos los cambios de velocidad del ventilador.\nTodos los ajustes se limitarán exactamente a este valor.',
            stepSizeFixedIncreasing: 'Aumentando',
            stepSizeFixedIncreasingTooltip:
                'Tamaño de paso fijo cuando la velocidad del ventilador está aumentando.\nTodos los ajustes ascendentes se limitarán exactamente a este valor.',
            stepSizeFixedDecreasing: 'Disminuyendo',
            stepSizeFixedDecreasingTooltip:
                'Tamaño de paso fijo cuando la velocidad del ventilador está disminuyendo.\nTodos los ajustes descendentes se limitarán exactamente a este valor.',
            stepSizeMinIncreasing: 'Mínimo Aumentando',
            stepSizeMinIncreasingTooltip:
                'Tamaño de paso mínimo cuando la velocidad del ventilador está aumentando.\nLos cambios calculados menores se ignoran para reducir ajustes innecesarios.',
            stepSizeMaxIncreasing: 'Máximo Aumentando',
            stepSizeMaxIncreasingTooltip:
                'Tamaño de paso máximo cuando la velocidad del ventilador está aumentando.\nLimita la rapidez con la que los ventiladores pueden acelerar por actualización.',
            stepSizeMinDecreasing: 'Mínimo Disminuyendo',
            stepSizeMinDecreasingTooltip:
                'Tamaño de paso mínimo cuando la velocidad del ventilador está disminuyendo.\nLos cambios calculados menores se ignoran para reducir ajustes innecesarios.',
            stepSizeMaxDecreasing: 'Máximo Disminuyendo',
            stepSizeMaxDecreasingTooltip:
                'Tamaño de paso máximo cuando la velocidad del ventilador está disminuyendo.\nLimita la rapidez con la que los ventiladores pueden desacelerar por actualización.',
            hysteresis: 'Histéresis Avanzada',
            hysteresisThreshold: 'Umbral',
            hysteresisThresholdTooltip:
                'Cambio mínimo de temperatura (°C) requerido antes de ajustar la velocidad del ventilador.\nAyuda a prevenir fluctuaciones rápidas de velocidad del ventilador por pequeñas variaciones de temperatura.',
            hysteresisDelay: 'Retraso',
            hysteresisDelayTooltip:
                'Retraso de respuesta (segundos) antes de aplicar cambios de velocidad del ventilador.\nLos picos de temperatura temporales dentro de este retraso se ignoran, suavizando las fluctuaciones.',
            onlyDownward: 'Solo Descendente',
            onlyDownwardTooltip:
                'Solo aplicar configuración de histéresis cuando la temperatura está disminuyendo.',
            stepOverrides: 'Anulaciones de paso',
            thresholdHopping: 'Salto de Umbral',
            thresholdHoppingTooltip:
                'Cuando la velocidad del ventilador permanece sin cambios durante 30+ segundos, los límites de tamaño de paso mínimo e histéresis se omiten temporalmente.\nEsto asegura que los ventiladores eventualmente alcancen su velocidad objetivo, incluso con configuraciones de umbral conservadoras. El tamaño de paso máximo siempre se respeta.',
            bypassMinAtExtremes: 'Aplicar siempre 0% / 100%',
            bypassMinAtExtremesTooltip:
                'Cuando está habilitado, los ciclos de trabajo objetivo de 0% o 100% se aplican incluso cuando el cambio es menor que el tamaño de paso mínimo.\nÚtil para garantizar que los ventiladores se detengan completamente o alcancen las RPM máximas. Deshabilitado por defecto.',
            unsavedChanges: 'Hay cambios no guardados realizados en esta Función.',
            unsavedChangesHeader: 'Cambios no guardados',
            functionError: 'Error al intentar actualizar esta función',
            newFunction: 'Nueva Función',
            functionDeleted: 'Función Eliminada',
            functionDuplicated: 'Función Duplicada',
            usedBy: 'Usada por',
            deleteFunctionConfirm: '¿Está seguro de que desea eliminar "{name}"?',
            deleteFunctionWithProfilesConfirm:
                '"{name}" está siendo utilizada actualmente por los Perfiles: {profiles}.\nEliminar esta Función restablecerá las Funciones de esos Perfiles.\n¿Está seguro de que desea eliminar "{name}"?',
        },
        error: {
            accessDenied: 'Acceso Denegado',
            accessDeniedMessage:
                'La autenticación falló. Por favor, verifique su contraseña e intente nuevamente.',
            connectionError: 'Error de Conexión CoolerControl',
            pageNotFound: 'Página No Encontrada',
            returnToDashboard: 'Volver al Panel',
            connectionErrorMessage: 'No se pudo conectar al Daemon CoolerControl.',
            serviceRunningMessage: 'Por favor, compruebe si el servicio daemon está en ejecución.',
            checkProjectPage: 'Para obtener ayuda configurando el daemon, consulte la',
            projectPage: 'página del proyecto',
            helpfulCommands: 'Comandos útiles:',
            nonStandardAddress:
                'Si tiene una dirección de daemon no estándar, puede especificarla a continuación:',
            daemonAddressDesktop: 'Dirección del Daemon (Aplicación de Escritorio)',
            daemonAddressWeb: 'Dirección del Daemon (Interfaz Web)',
            addressTooltip: 'La dirección IP o nombre de dominio para establecer una conexión.',
            portTooltip: 'El puerto para establecer una conexión.',
            sslTooltip: 'Si conectarse al daemon usando SSL/TLS.',
            saveTooltip: 'Guardar configuración y recargar la interfaz de usuario',
            resetTooltip: 'Restablecer a la configuración predeterminada',
        },
        mode: {
            activateMode: 'Activar modo',
            currentlyActive: 'Actualmente activo',
            modeHint:
                'Nota: Los modos no incluyen configuraciones de Perfil o Función, solo configuraciones de canal.',
        },
        lighting: {
            saveLightingSettings: 'Guardar configuración de iluminación',
            lightingMode: 'Modo de iluminación',
            speed: 'Velocidad',
            direction: 'Dirección',
            forward: 'Adelante',
            backward: 'Atrás',
            numberOfColors: 'Número de colores',
            numberOfColorsTooltip: 'Número de colores a usar para el modo de iluminación elegido.',
        },
        lcd: {
            saveLcdSettings: 'Guardar configuración de LCD',
            lcdMode: 'Modo LCD',
            brightness: 'Brillo',
            brightnessPercent: 'Porcentaje de brillo',
            orientation: 'Orientación',
            orientationDegrees: 'Orientación en grados',
            chooseImage: 'Elegir imagen',
            dragAndDrop: 'Arrastre y suelte archivos aquí.',
            tempSource: 'Fuente de temperatura',
            tempSourceTooltip: 'Fuente de temperatura para usar en la pantalla LCD.',
            imagesPath: 'Ruta de imágenes',
            imagesPathTooltip:
                'Introduzca la ruta absoluta al directorio que contiene las imágenes.\nEl directorio debe contener al menos un archivo de imagen, y pueden\nser imágenes estáticas o gifs. El Carrusel los recorrerá\ncon el retraso seleccionado. Todos los archivos se procesan\nal enviarse para garantizar la máxima compatibilidad.',
            browse: 'Explorar',
            browseTooltip: 'Explorar un directorio de imágenes',
            delayInterval: 'Intervalo de retraso',
            delayIntervalTooltip:
                'Número mínimo de segundos de retraso entre cambios de imagen.\nTenga en cuenta que el retraso real puede ser mayor debido a la tasa de sondeo del daemon.',
            processing: 'Procesando...',
            applying: 'Aplicando...',
            unsavedChanges: 'Hay cambios no guardados realizados en esta configuración de LCD.',
            unsavedChangesHeader: 'Cambios no guardados',
            imageTooLarge: 'La imagen es demasiado grande. Por favor, elija una más pequeña.',
            notImageType: 'La imagen no se registra como un tipo de imagen',
            gifNotSupported:
                'El firmware de esta pantalla no puede mostrar gifs. Elige una imagen estática.',
        },
        shortcuts: {
            browserHint:
                'En un navegador web, use Ctrl+Alt+número en su lugar (los navegadores reservan Ctrl+número para cambiar de pestaña).',
            shortcuts: 'Atajos de teclado',
            ctrl: 'Ctrl',
            comma: ',',
            viewShortcuts: 'Atajos de teclado',
            settings: 'Ajustes',
        },
    },
    components: {
        aseTek690: {
            sameDeviceID:
                'Los NZXT Kraken antiguos y los EVGA CLC tienen el mismo ID de dispositivo y CoolerControl no puede determinar qué dispositivo está conectado. Esto es necesario para una comunicación adecuada con el dispositivo.',
            restartRequired:
                'Puede ser necesario reiniciar los servicios systemd de CoolerControl y se manejará automáticamente si es necesario.',
            deviceModel: '¿Es el dispositivo Liquidctl uno de los siguientes modelos?',
            modelList: 'NZXT Kraken X40, X60, X31, X41, X51 o X61',
            acceptLabel: 'Sí, es un dispositivo Kraken antiguo',
            rejectLabel: 'No, es un dispositivo EVGA CLC',
        },
        password: {
            forgotPassword: '¿Olvidó su contraseña?',
            forgotPasswordHelpIntro:
                'Ejecute este comando en una terminal como root y luego haga clic en Recargar UI:',
            forgotPasswordCopyCommand: 'Copiar comando',
            forgotPasswordCommandCopied: 'Comando copiado al portapapeles',
            forgotPasswordReloadButton: 'Recargar UI',
            continueButton: 'Continuar',
            backButton: 'Atrás',
            passwordMismatch: 'Las contraseñas no coinciden',
        },
        notFound: {
            message: 'Al igual que la distribución perfecta de Linux 🐧,\nesta página no existe.',
        },
        deviceInfo: {
            details: 'Detalles del Dispositivo',
            systemName: 'Nombre del Sistema',
            deviceType: 'Tipo de Dispositivo',
            deviceUID: 'UID del Dispositivo',
            firmwareVersion: 'Versión de Firmware',
            model: 'Modelo',
            driverName: 'Nombre del Controlador',
            driverType: 'Tipo de Controlador',
            driverVersion: 'Versión del Controlador',
            locations: 'Ubicaciones',
        },
        onboarding: {
            search: 'Búsqueda',
            searchDesc:
                'Encuentre aquí cualquier dispositivo, sensor, ajuste o acción. Pulse Ctrl+K desde cualquier parte de la aplicación.',
            welcome: '¡Bienvenido a CoolerControl!',
            gettingStartedIntro:
                'Haga un recorrido rápido para orientarse. Recorre la barra de navegación y las áreas principales de la aplicación.',
            startTourAgain:
                'Puede volver a iniciar este recorrido en cualquier momento desde Configuración.',
            startTour: 'Iniciar Recorrido',
            maybeLater: 'Quizás más tarde',
            openGettingStarted: 'Abrir Documentación de Inicio',
            finishLater: 'Entendido, gracias',
            home: 'Inicio',
            homeDesc:
                'La página de inicio de la aplicación: estado del daemon y salud de los dispositivos de un vistazo, además de registros, información de la app, enlaces útiles y herramientas de prueba de estrés.',
            cooling: 'Refrigeración',
            coolingDesc:
                'Su centro de control de ventiladores: ajuste las velocidades de ventiladores y bombas, y aplique Perfiles y Funciones a cualquier canal.',
            monitoring: 'Monitoreo',
            monitoringDesc:
                'Cree Paneles, observe cada sensor y configure Alertas para monitorear su sistema en tiempo real.',
            devices: 'Dispositivos',
            devicesDesc:
                'Revise el hardware detectado, configure funciones por dispositivo como iluminación RGB y pantallas LCD, y cree Sensores Personalizados.',
            plugins: 'Plugins',
            pluginsDesc: 'Explore y abra los plugins instalados que amplían CoolerControl.',
            settings: 'Configuración',
            settingsDesc:
                'Configure preferencias de interfaz, opciones del daemon y comportamiento del sistema.',
            access: 'Acceso',
            accessDesc:
                'Inicie o cierre sesión y cambie su contraseña, y gestione los Tokens de acceso que dan a herramientas y plugins acceso a la API.',
            restartMenu: 'Menú de Reinicio',
            restartMenuDesc:
                'Recargue la interfaz o reinicie el daemon del sistema cuando sea necesario.',
            modes: 'Modos',
            modesDesc:
                'Los Modos son colecciones guardadas de sus configuraciones. Cambie aquí entre configuraciones como Silencioso y Rendimiento, o gestiónelas.',
            thatsIt: '¡Eso es todo!',
            startNow:
                'Ya está todo listo. Abra la documentación de Inicio para aprender más, o comience a configurar sus dispositivos.',
        },
        axisOptions: {
            title: 'Opciones de Ejes',
            autoScale: 'AutoEscala',
            max: 'Máx',
            min: 'Mín',
            dutyTemperature: 'Ciclo / Temperatura',
            rpmMhz: 'rpm / MHz',
            krpmGhz: 'krpm / GHz',
            watts: 'vatios',
        },
        sensorTable: {
            device: 'Dispositivo',
            channel: 'Canal',
            current: 'Actual',
            range: 'Rango',
            average: 'Promedio',
            resetStats: 'Restablecer',
            resetStatsTooltip: 'Restablecer mín/máx/promedio para todos los canales',
        },
        modeTable: {
            setting: 'Configuración',
        },
        menuTagAssign: {
            title: 'Asignar Etiquetas',
            noTags: 'No hay etiquetas aún.',
            tagName: 'Nombre de etiqueta',
            editTag: 'Editar etiqueta',
            deleteTag: 'Eliminar etiqueta',
        },
        wizards: {
            calibration: {
                title: 'Calibrar ventiladores',
                pickIntro:
                    'Seleccione los ventiladores a calibrar. Los ya calibrados y los controlados por firmware están desmarcados de forma predeterminada.',
                noFans: 'No se detectaron ventiladores controlables.',
                selectAll: 'Seleccionar todo',
                calibratedBadge: 'calibrado',
                firmwareControlledBadge: 'controlado por firmware',
                firmwareControlledDesc:
                    'El firmware ejecuta el perfil de este canal. La calibración sigue aplicándose: su conversión de ciclo de trabajo se incorpora a la curva entregada al firmware. El impulso de arranque no, porque una curva de firmware no puede expresarlo.',
                blockedByAlert: "bloqueado: la alerta '{name}' está activa",
                alertsPausedNote:
                    '{count} alerta(s) vigilan los ventiladores seleccionados y se pausan durante el barrido de cada ventilador.',
                idleNote:
                    'La calibración recorre todo el rango de cada ventilador. Es mejor ejecutarla en reposo: hace ruido y tarda unos minutos por ventilador.',
                concurrencyLabel: 'Ventiladores a la vez',
                concurrencyNote:
                    'Hacer más a la vez es más rápido, pero los ventiladores adyacentes pueden distorsionar las lecturas de los demás (corrientes cruzadas, push-pull). Uno a la vez es lo más preciso.',
                start: 'Iniciar',
                close: 'Cerrar',
                running: 'Calibrando {current} de {total}...',
                queued: 'En cola',
                done: 'Hecho',
                failed: 'Fallido',
                skipped: 'Omitido',
                startFailed: 'No se pudo iniciar',
                summary: '{done} calibrados, {failed} fallidos, {skipped} omitidos.',
                reloadBatch:
                    '{count} ventiladores calibrados. ¿Recargar para aplicar el nuevo control normalizado por RPM?',
                stagePreflight: 'Comprobación previa',
                stageUpSweep: 'Barrido ascendente',
                stageDownSweep: 'Barrido descendente',
                stageFinalizing: 'Finalizando',
            },
            fanControl: {
                fanControlWizard: 'Asistente de Control de Ventiladores',
                editCurrentProfile: 'Editar Perfil',
                editCurrentFunction: 'Editar Función',
                currentSettings: 'Configuración Actual',
                manualSpeed: 'Velocidad Manual',
                createNewProfile: 'Nuevo Perfil',
                existingProfile: 'Elegir Perfil',
                resetSettings: 'Restablecer a Sin gestión',
                chooseProfileNameType: 'Elegir un Nombre y Tipo de Perfil',
                newDefaultProfile: 'Nuevo Perfil Predeterminado',
                profileCreatedApplied: 'Perfil Creado y Aplicado',
                willCreatedAndAppliedTo: 'será creado y aplicado a',
                newFixedProfile: 'Nuevo Perfil Fijo',
                withSettings: 'con la siguiente configuración',
                selectSpeed: 'Seleccione su velocidad',
                newMixProfile: 'Nuevo Perfil de Mezcla',
                newGraphProfile: 'Nuevo Perfil Gráfico',
                newOverlayProfile: 'Nuevo Perfil de Superposición',
                functionFor: 'Elija una Función para aplicar a',
                functionDescription:
                    'Las funciones ajustan cómo se aplica su Perfil, como el tiempo de respuesta y la velocidad mínima.',
                createNewFunction: 'Nueva Función',
                existingFunction: 'Elegir Función',
                defaultFunction: 'Función Predeterminada',
                chooseFunctionName: 'Elige un nombre para la función',
                newFunctionName: 'Función para {profileName}',
                summary: 'Resumen',
                aNewProfile: 'Un nuevo Perfil',
                andFunction: 'y Función',
            },
            profile: {
                willCreated: 'será creado.',
            },
            profileApply: {
                applyProfile: 'Aplicar Perfil',
                channelsApply: 'Canales para Aplicar Perfil',
                selectChannels: 'Seleccionar Canales',
                channelsTooltip: 'Seleccione uno o más canales para aplicar este Perfil.',
                selectByTag: 'Seleccionar por etiqueta',
                selectByChannel: 'Seleccionar por canal',
                tagFanCount: '{count} canal | {count} canales',
                noTags: 'No hay etiquetas configuradas.',
            },
            functionApply: {
                applyFunction: 'Aplicar Función',
                profilesApply: 'Perfiles para Aplicar Función',
                selectProfiles: 'Seleccionar Perfiles',
                profilesTooltip: 'Seleccione uno o más Perfiles para aplicar esta Función.',
            },
            generate: {
                title: 'Crear perfiles automáticamente',
                assignIntro:
                    'Asigne una función a cada ventilador. Deje un ventilador sin asignar para omitirlo.',
                calibrateFirst:
                    'Calibre primero los ventiladores para una mayor uniformidad (unos minutos)',
                skip: 'Omitir',
                noFans: 'No se detectaron ventiladores controlables.',
                tempsIntro:
                    'Elija las temperaturas que debe seguir su configuración. Deje una vacía para excluirla: un sistema con gráficos integrados no necesita temp. GPU, y elegirla es lo que hace participar a la GPU en las curvas del radiador AIO y de los ventiladores de la caja.',
                cpuTemp: 'Temp. CPU',
                gpuTemp: 'Temp. GPU',
                liquidTemp: 'Temp. del líquido',
                ambientTemp: 'Temp. ambiente (opcional)',
                tempNone: 'Ninguna',
                presetIntro: 'Elija con qué agresividad deben acelerar los ventiladores.',
                perKindOverrides: 'Anulaciones por función (avanzado)',
                cfmCaveat:
                    'El sesgo de presión positiva se basa en el ciclo de trabajo (duty), no en el flujo de aire: con cantidades de ventiladores desiguales no puede garantizar presión positiva.',
                previewIntro:
                    'Revise lo que se creará y aplicará. No se guarda nada hasta que confirme.',
                previewAssignments: 'Asignaciones de ventiladores',
                reusedHeader: 'Ya existe',
                reused: 'reutilizado',
                willCreateHeader: 'Se creará',
                startingPointNote:
                    'Un punto de partida general para su configuración de ventiladores, pensado para ajustarse y no para dejarse tal cual.',
                replaces: 'reemplaza {name}',
                generated: '{count} perfiles generados.',
                generateError: 'No se pudieron generar los perfiles.',
                applyError: 'No se pudieron crear los perfiles.',
                kind: {
                    CpuCooler: 'Refrigerador de aire de CPU',
                    GpuFan: 'Ventilador de GPU',
                    AioRadiator: 'Radiador AIO',
                    AioPump: 'Bomba AIO',
                    CaseIntake: 'Entrada de la caja',
                    CaseExhaust: 'Salida de la caja',
                    LaptopFan: 'Ventilador de portátil',
                },
            },
        },
        channelExtensionSettings: {
            title: 'Ajustes del canal del dispositivo',
            firmwareControlledProfile: 'Perfil controlado por firmware',
            firmwareControlledProfileDesc:
                'Cuando está habilitado, el firmware del dispositivo gestiona el perfil del ventilador.\nÚtil para hardware que no responde bien a cambios de velocidad frecuentes realizados por software.\nSolo disponible para Perfiles de gráfico que utilizan sensores de temperatura internos del dispositivo.\nLos ajustes de Función no se aplican.\nEn un canal calibrado, los puntos de la curva se convierten mediante la calibración, pero el impulso de arranque no se aplica.',
            saveError: 'Error al guardar los ajustes de la extensión del canal',
            firmwareControlDisabled:
                'El control por firmware no está disponible con la configuración actual.\nUse un Perfil de gráfico para este dispositivo con un sensor de temperatura interno compatible.',
            calibration: {
                heading: 'Calibración de RPM',
                description:
                    'Recorra el ventilador para obtener su curva real de ciclo de trabajo a RPM y, a continuación, controle el canal como ciclo de trabajo real normalizado por RPM.\nElimina zonas muertas a ciclo bajo y la saturación a ciclo alto.\nEl impulso de arranque también se gestiona automáticamente cuando el ventilador está calibrado: un breve refuerzo inicial lo pone en marcha desde reposo antes de estabilizarlo en el ciclo objetivo.\nEl barrido suele tardar varios minutos y puede ser notablemente más largo en ventiladores de respuesta lenta. El canal se ajusta a 0 % al inicio.',
                statusNotCalibrated: 'Sin calibrar',
                blockedByAlert:
                    "La calibración está bloqueada: la alerta '{name}' está activa en este ventilador.",
                alertsPausedNote:
                    'Las alertas que vigilan este ventilador se pausan durante el barrido.',
                statusInProgress: 'Calibrando: {stage} ({percent} %)',
                statusCompleted: 'Calibrado (curva continua, mapeo activo)',
                statusCompletedStepped: 'Calibrado (curva escalonada, mapeo desactivado)',
                statusCompletedWithWarnings: 'Calibrado con advertencias: {messages}',
                statusFailed: 'El último intento falló: {message}',
                warningNoTachometer:
                    'no se detectaron RPM (el sensor o el cableado pueden estar desconectados)',
                warningNotControllable:
                    'el ventilador no responde al ciclo de trabajo (probablemente controlado por la BIOS)',
                warningLimitedRange:
                    'rango de RPM limitado ({span} RPM); la resolución del mapeo es gruesa',
                warningOscillating:
                    'el ventilador oscila entre {lower} % y {upper} % de ciclo (impulso controlado por firmware); mapeo desactivado a ciclo bajo',
                stagePreflight: 'previo',
                stageUpSweep: 'barrido ascendente',
                stageDownSweep: 'barrido descendente',
                stageFinalizing: 'finalizando',
                buttonCalibrate: 'Calibrar',
                buttonRecalibrate: 'Recalibrar',
                buttonCancel: 'Cancelar',
                buttonClear: 'Borrar',
                clearConfirm:
                    '¿Borrar la calibración de {channel}? Volver a ejecutarla tarda varios minutos.',
                buttonViewCurve: 'Ver curva',
                caveatsBanner:
                    'Calibrar varios ventiladores de refrigeración principales a la vez puede aumentar la temperatura del sistema.\nLos ventiladores push-pull de un radiador diagnosticados en paralelo pueden generar lecturas inexactas.\nMantenga el sistema inactivo durante la calibración.',
                clearedNotice:
                    'Borrado. Las curvas de ventilador de este canal ahora controlan directamente el ciclo de trabajo del dispositivo.',
                startError: 'No se pudo iniciar la calibración',
                cancelError: 'No se pudo cancelar la calibración',
                clearError: 'No se pudo borrar la calibración',
                reloadHeader: 'Recargar interfaz',
                reloadAccept: 'Recargar',
                reloadReject: 'Más tarde',
                reload_rpm_only_completed_single:
                    'Calibración completada para {channelName}. Recargue la interfaz para mostrar el gráfico de ciclo del canal.',
                reload_rpm_only_completed_multi:
                    'Calibración completada para {channelList}. Recargue la interfaz para mostrar el gráfico de ciclo de cada canal.',
                reload_rpm_only_cleared_single:
                    'Calibración borrada para {channelName}. Recargue la interfaz para eliminar el gráfico de ciclo del canal, ahora obsoleto.',
                reload_rpm_only_cleared_multi:
                    'Calibración borrada para {channelList}. Recargue la interfaz para eliminar el gráfico de ciclo de cada canal, ahora obsoleto.',
                reload_duty_range_completed_single:
                    'Calibración completada para {channelName}. Recargue la interfaz para que el control manual de ciclo y el asistente de control del ventilador adopten el nuevo rango de ciclo del canal.',
                reload_duty_range_completed_multi:
                    'Calibración completada para {channelList}. Recargue la interfaz para que el control manual de ciclo y el asistente de control del ventilador adopten el nuevo rango de ciclo de cada canal.',
                reload_duty_range_cleared_single:
                    'Calibración borrada para {channelName}. Recargue la interfaz para que el control manual de ciclo vuelva a los límites de hardware del canal.',
                reload_duty_range_cleared_multi:
                    'Calibración borrada para {channelList}. Recargue la interfaz para que el control manual de ciclo vuelva a los límites de hardware de cada canal.',
                reload_mixed_multi:
                    'Calibración modificada para {channelList}. Recargue la interfaz para que cada canal adopte la nueva visualización de ciclo y los límites del control.',
            },
        },
        calibrationCurve: {
            dialogTitle: 'Curva de calibración',
            loading: 'Cargando calibración...',
            notFound: 'No se encontraron datos de calibración para este canal.',
            loadError: 'No se pudieron cargar los datos de calibración.',
            axisDuty: 'Ciclo',
            axisRpm: 'RPM',
            legendUp: 'Barrido ascendente',
            legendDown: 'Barrido descendente',
            markerStable: 'Piso estable',
            curveKindSmooth: 'Continua (mapeo activo)',
            curveKindStepped: 'Escalonada (mapeo desactivado)',
            fieldCurveKind: 'Curva',
            fieldCurveKindTooltip:
                'Cómo responde el canal a los cambios de ciclo.\nLos ventiladores continuos tienen una curva ciclo-a-RPM continua, por lo que el dispatcher mapea el ciclo objetivo a través de la calibración. Los ventiladores escalonados tienen plataformas de RPM discretas, por lo que los ciclos pasan sin modificación.',
            fieldRpmMax: 'RPM máximas',
            fieldRpmMaxTooltip:
                'RPM máximas observadas durante el barrido.\nSe usa como referencia de 100% al traducir un ciclo objetivo a su valor real normalizado por RPM.',
            fieldKick: 'Duración del impulso',
            fieldKickTooltip:
                'Cuánto tiempo el dispatcher mantiene el ciclo de impulso antes de bajar al sostenimiento en un arranque en frío.\nMedido escribiendo el ciclo de impulso de peor caso (con boost) del dispatcher desde reposo y esperando hasta que las RPM se asienten en una ventana estable.',
            fieldStart: 'Ciclo mínimo de arranque',
            fieldStartTooltip:
                'Ciclo más bajo que arranca el ventilador de forma fiable desde detenido.\nPor debajo, el ventilador puede no comenzar a girar aunque seguiría girando si ya estuviera en marcha.',
            fieldSustain: 'Ciclo mínimo de sostenimiento',
            fieldSustainTooltip:
                'Ciclo más bajo en el que el ventilador sigue girando una vez arrancado.\nEl dispatcher no bajará el ciclo en marcha por debajo de este valor, salvo que el canal se envíe a 0.',
            fieldStable: 'Ciclo mínimo estable',
            fieldStableTooltip:
                'Ciclo más bajo en el que el ventilador funciona sin oscilación.\nLos ventiladores controlados por firmware elevan las RPM por encima de un piso interno a ciclo bajo, generando un aleteo audible; el dispatcher limita el sostenimiento posterior al impulso a este valor para que el ventilador permanezca por encima de la banda.',
            fieldSaturate: 'Ciclo cerca del plateau',
            fieldSaturateTooltip:
                'Ciclo a partir del cual las ganancias de RPM comienzan a disminuir.\nEl ventilador puede seguir añadiendo algunas RPM más allá de este ciclo hasta el 100 %, por lo que la calibración utiliza todo el rango de 0 a 100 %.',
            fieldTimestamp: 'Calibrado',
            overridesHeading: 'Sobrescrituras',
            fieldKickBoostOverride: 'Boost de impulso',
            fieldKickBoostOverrideTooltip:
                'Fuerza la activación o desactivación del boost de impulso de arranque en frío para este canal, o deja que el daemon decida según la heurística de la curva ascendente.\nEl boost eleva brevemente el ciclo de impulso por encima del sostenimiento para empujar al ventilador más allá de su umbral de inercia.',
            kickBoostAuto: 'Auto',
            kickBoostOn: 'Forzar activado',
            kickBoostOff: 'Forzar desactivado',
            fieldKickDurationOverride: 'Sobrescritura de la duración del impulso',
            fieldKickDurationOverrideTooltip:
                'Sobrescribe la duración del impulso calibrada. Deja vacío para usar el valor medido.\nAlarga cuando el ventilador necesita más tiempo en el ciclo de impulso para estabilizarse antes de que tome el relevo el sostenimiento.',
            kickDurationDefault: 'predeterminado',
            kickDurationReset: 'Restablecer al predeterminado',
            kickBoostCurrentlyOn: 'actualmente activado',
            kickBoostCurrentlyOff: 'actualmente desactivado',
            fieldWalkAfterKick: 'Descenso gradual tras el impulso',
            fieldWalkAfterKickTooltip:
                'Tras la ventana de impulso, reduce el ciclo de trabajo hacia el sostenimiento en pequeños incrementos. Protege los ventiladores cuyos controladores cortan la corriente ante una caída brusca.\nDesactivar para saltar directamente del impulso al sostenimiento. Seguro en la mayoría de los ventiladores PWM modernos y elimina la rampa descendente visible tras cada arranque en frío.',
            overridesSaveFailed: 'Error al guardar las sobrescrituras de calibración',
        },
        deviceExtensionSettings: {
            title: 'Configuración Avanzada del Dispositivo',
            directAccess: 'Acceso Directo',
            directAccessDesc:
                'Cuando está habilitado, el controlador liquidctl ignorará el controlador del kernel HWMon\ny se comunicará directamente con el dispositivo.\nEsto puede ser útil para dispositivos que tienen conflictos al usar ambos controladores.',
            useHwmon: 'Usar controlador HWMon',
            useHwmonDesc:
                'Cambia el controlador de este dispositivo de liquidctl al controlador del kernel HWMon.\nEsto puede mejorar el rendimiento y la estabilidad, pero puede reducir las funciones disponibles.',
            disableDevice: 'Deshabilitar dispositivo liquidctl',
            disableInfo:
                'Deshabilitar el controlador liquidctl deshabilitará este dispositivo. Aparecerá un nuevo dispositivo basado en HWMon en la parte inferior del menú de dispositivos. Puede volver a habilitar el dispositivo liquidctl en cualquier momento desde el menú de configuración.',
            commandDelay: 'Retardo de comando',
            commandDelayDesc:
                'Retardo en milisegundos entre comandos enviados a este dispositivo.\nEsto puede ayudar con dispositivos que tienen problemas de comunicación\ncuando se envían múltiples comandos en rápida sucesión.',
            overdrive: 'GPU Overdrive',
            overdriveDesc:
                'Las GPU AMD RDNA3/4 requieren que overdrive esté habilitado para el control de ventiladores.\nEsto configura el parámetro del kernel amdgpu.ppfeaturemask\ny requiere un reinicio del sistema.',
            overdriveEnable: 'Habilitar',
            overdriveActive: 'Activo',
            overdriveSuccess: 'Overdrive configurado',
            thinkPadFanControl: 'Control del ventilador',
            thinkPadFanControlDesc:
                'Habilita el control del ventilador ACPI de ThinkPad.\nEl control del ventilador está deshabilitado por defecto por razones de seguridad.\nProceda bajo su propio riesgo.',
            thinkPadFullSpeed: 'Velocidad máxima',
            thinkPadFullSpeedDesc:
                'Habilita el modo de velocidad máxima para los ventiladores ThinkPad.\nPermite que los ventiladores giren al máximo absoluto al 100%,\npero opera los ventiladores fuera de especificación con mayor desgaste.',
        },
    },
    auth: {
        enterPassword: 'Introduzca Su Contraseña',
        setNewPassword: 'Introduzca Una Nueva Contraseña',
        changeDefaultPassword:
            'Por favor, establezca una contraseña para prevenir el acceso no autorizado. Esta es independiente de su cuenta del sistema.',
        accessTokens: 'Tokens de acceso',
        tokenLabel: 'Etiqueta (ej. cctv)',
        tokenExpiry: 'Fecha de expiración (opcional)',
        createToken: 'Crear token',
        tokenCreated: 'Token creado',
        tokenCreatedDetail: 'Copie este token ahora. No se mostrará de nuevo.',
        tokenCopied: 'Token copiado al portapapeles',
        tokenDeleted: 'Token eliminado',
        tokenCreateError: 'Error al crear el token',
        tokenDeleteError: 'Error al eliminar el token',
        tokenLoadError: 'Error al cargar los tokens',
        tokenDeleteConfirm:
            '¿Está seguro de que desea eliminar este token? Los servicios que lo usen perderán el acceso.',
        tokenDeleteHeader: 'Eliminar token',
        noTokens: 'Aún no se han creado tokens de acceso.',
        expires: 'Expira',
        expired: 'Expirado',
        active: 'Activo',
        never: 'Nunca',
        lastUsed: 'Último uso',
        neverUsed: 'Nunca usado',
        created: 'Creado',
        label: 'Etiqueta',
        actions: 'Acciones',
        writeAccess: 'Acceso de escritura',
        writeAccessTooltip:
            'Cuando está activado, este token puede realizar cambios. Cuando está desactivado, el token solo puede leer datos.',
    },
    daemon: {
        status: {
            ok: 'Ok',
            hasWarnings: 'Tiene Advertencias',
            hasErrors: 'Tiene Errores',
        },
    },
    // Rendered by the Qt desktop app, which has no translation pipeline of its own.
    // Pushed over IPC and cached there. See shell/qtStrings.ts.
    desktop: {
        closePrompt: {
            title: '¿Cerrar a la bandeja del sistema?',
            body: 'El demonio de CoolerControl sigue funcionando en segundo plano en cualquier caso, por lo que su configuración de refrigeración permanece activa. Mantenga la interfaz en la bandeja del sistema para un acceso rápido y notificaciones de escritorio, o ciérrela por completo.',
            keepInTray: 'Mantener en la bandeja',
            quit: 'Salir',
            remember: 'Recordar mi elección',
        },
        tray: {
            show: '&Mostrar',
            hide: '&Ocultar',
            daemonConnection: 'Conexión del &demonio…',
            quit: '&Salir',
            modes: 'Modos',
            sensors: 'Sensores',
            daemons: 'Demonios',
        },
        cert: {
            title: 'Certificado del demonio no verificado',
            changedTitle: 'El certificado ha cambiado',
            // %1 is the daemon host, substituted by Qt via QString::arg.
            body: '%1 usa un certificado autofirmado que no se puede verificar automáticamente. Continúe solo si reconoce este demonio.',
            changedBody:
                'El certificado de %1 no es el que se aprobó anteriormente. Puede significar que el demonio se reinstaló o que algo está interceptando la conexión.',
            fingerprint: 'Huella digital (SHA-256):',
            trust: 'Confiar en este certificado',
            cancel: 'Cancelar',
        },
        wizard: {
            windowTitle: 'Error de conexión con el demonio',
            windowTitleOk: 'Conexión del demonio',
            apply: '&Aplicar',
            retry: '&Reintentar',
            quitApp: '&Salir',
            introPurpose:
                'Esta configuración controla cómo la aplicación de escritorio se conecta al demonio de CoolerControl.',
            introFailed: 'No se pudo establecer una conexión con el demonio de CoolerControl.',
            introCheckService:
                'Asegúrese de que el servicio de systemd esté en ejecución y disponible.',
            // %1 is substituted by Qt via QString::arg, not by vue-i18n.
            introDocs: 'Consulte el %1 para ver las instrucciones de instalación.',
            introDocsLink: 'sitio de documentación',
            introCommands:
                'Algunos comandos útiles para activar y verificar el estado del demonio:',
            introCustomAddress:
                'Si ha configurado una dirección no estándar para conectarse al demonio, puede establecerla en los siguientes pasos:',
            lastError: 'Último error:',
            // %1 is substituted by Qt via QString::arg, not by vue-i18n.
            errorNotDaemon:
                'La dirección respondió, pero no como un demonio de CoolerControl (HTTP %1).',
            errorCertUntrusted: 'No se confió en el certificado del demonio.',
            errorCertInvalid:
                'El certificado del demonio no es válido y la validación de certificados está activada.',
            savedLabel: 'Conexión guardada:',
            newConnection: 'Nueva conexión…',
            removeConnection: 'Eliminar',
            removeConnectionTooltip: 'Olvidar el demonio seleccionado.',
            removeConnectionBody: '¿Dejar de ofrecer este demonio en la bandeja?',
            nameLabel: 'Nombre:',
            nameTooltip: 'Etiqueta opcional para este demonio. En blanco muestra host:puerto.',
            addressTitle: 'Dirección del demonio - Aplicación de escritorio',
            addressSubtitle: 'Ajuste los campos de dirección según sea necesario.',
            hostLabel: 'Dirección del host:',
            hostTooltip:
                'La dirección IPv4, IPv6 o el nombre de host que se usará para comunicarse con el demonio.',
            portLabel: 'Puerto:',
            portTooltip: 'El número de puerto que se usará para comunicarse con el demonio.',
            sslTooltip: 'Activar o desactivar SSL/TLS (HTTPS)',
            strictTls: 'Validar certificado',
            strictTlsTooltip:
                'Exigir un certificado que se valide normalmente. Déjelo desactivado para usar el certificado autofirmado del demonio, que se aprueba en el primer uso para demonios remotos.',
            defaults: 'Valores predeterminados',
            defaultsTooltip: 'Restablecer la dirección del demonio a los valores predeterminados',
            forgetCerts: 'Olvidar certificados de confianza',
            forgetCertsTooltip:
                'Elimina los certificados de demonios remotos en los que esta aplicación confía.',
            forgetCertsBody:
                'Actualmente se confía en estos certificados de demonio. Olvidarlos significa que se le pedirá confirmación la próxima vez que se conecte.',
        },
        versionMismatch: {
            title: 'Versiones incompatibles',
            text: 'La versión de la aplicación de escritorio (%1) no coincide con la versión del demonio (%2).',
            informative:
                'Reinicie la aplicación de escritorio para cargar la versión correcta de la interfaz.',
            quitApp: '&Salir',
            continueAnyway: 'Continuar de todos modos',
        },
    },
    device_store: {
        unauthorized: {
            summary: 'Sesión expirada',
            detail: 'Su sesión ha expirado. Recargando para iniciar sesión nuevamente.',
        },
        login: {
            failed: {
                summary: 'Inicio de Sesión Fallido',
                detail: 'Contraseña Inválida',
            },
            rate_limited: {
                summary: 'Inicio de Sesión Temporalmente Bloqueado',
            },
        },
        logout: {
            summary: 'Cierre de Sesión',
            detail: 'Ha cerrado sesión correctamente.',
        },
        password: {
            set_success: {
                summary: 'Contraseña',
                detail: 'Nueva contraseña establecida correctamente',
            },
        },
        asetek: {
            header: 'Dispositivo Desconocido Detectado',
            success: {
                summary: 'Éxito',
                detail_legacy:
                    'Tipo de modelo de dispositivo establecido correctamente. Reinicio en progreso.',
                detail_evga: 'Tipo de modelo de dispositivo establecido correctamente.',
            },
            error: {
                summary: 'Error',
                detail: 'Proceso interrumpido.',
            },
        },
    },
    models: {
        chartType: {
            timeChart: 'Gráfico de Tiempo',
            table: 'Tabla',
        },
        dataType: {
            temp: 'Temp',
            duty: 'Ciclo',
            load: 'Carga',
            rpm: 'RPM',
            freq: 'Frec',
            watts: 'Vatios',
        },
        profile: {
            profileType: {
                default: 'Predeterminado',
                fixed: 'Fijo',
                graph: 'Gráfico',
                mix: 'Mezcla',
                overlay: 'Superposición',
            },
            mixFunctionType: {
                min: 'Mínimo',
                max: 'Máximo',
                avg: 'Promedio',
                diff: 'Diferencia',
                sum: 'Suma',
            },
        },
        customSensor: {
            sensorType: {
                mix: 'Mezcla',
                file: 'Archivo',
                offset: 'Desplazamiento',
                timeAverage: 'Promedio Temporal',
                exponentialMovingAvg: 'Promedio Móvil Exponencial',
            },
            mixFunctionType: {
                min: 'Mínimo',
                max: 'Máximo',
                delta: 'Delta',
                avg: 'Promedio',
                weightedAvg: 'Promedio Ponderado',
            },
        },
        themeMode: {
            system: 'Sistema',
            dark: 'Oscuro',
            light: 'Claro',
            highContrastDark: 'Oscuro de Alto Contraste',
            highContrastLight: 'Claro de Alto Contraste',
            custom: 'Tema Personalizado',
        },
        interfaceFont: {
            bundled: 'Incluida (IBM Plex)',
            system: 'Sistema',
        },
        channelViewType: {
            control: 'Control',
            dashboard: 'Panel',
        },
        startupPage: {
            appInfo: 'Info y Herramientas',
            homeDashboard: 'Panel principal',
            controls: 'Controles',
        },
        alertState: {
            active: 'Activo',
            inactive: 'Inactivo',
            error: 'Error',
        },
        pluginStatus: {
            running: 'En ejecución',
            stopped: 'Detenido',
            unmanaged: 'No gestionado',
            disabled: 'Deshabilitado',
        },
        deviceType: {
            customSensors: 'Sensores Personalizados',
            cpu: 'CPU',
            gpu: 'GPU',
            liquidctl: 'Liquidctl',
            hwmon: 'Hwmon',
            servicePlugin: 'Complemento de Servicio',
        },
        driverType: {
            kernel: 'Kernel',
            liquidctl: 'Liquidctl',
            nvml: 'NVML',
            nvidiaCli: 'Nvidia CLI',
            coolercontrol: 'CoolerControl',
            external: 'Externo',
        },
        lcdModeType: {
            none: 'Ninguno',
            liquidctl: 'Liquidctl',
            custom: 'Personalizado',
        },
        channelType: {
            lcd: 'LCD',
        },
    },
}

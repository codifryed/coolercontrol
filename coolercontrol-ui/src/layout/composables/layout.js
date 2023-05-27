import {toRefs, reactive, computed} from 'vue';

const layoutConfig = reactive({
    ripple: true,
    darkTheme: true,
    inputStyle: 'outlined',
    menuMode: 'static',
    theme: 'lara-dark-indigo',
    scale: 14,
    activeMenuItem: null
});

const layoutState = reactive({
    staticMenuDesktopInactive: false,
    overlayMenuActive: false,
    profileSidebarVisible: false,
    configSidebarVisible: false,
    staticMenuMobileActive: false,
    menuHoverActive: false
});

export function useLayout() {
    const changeThemeSettings = (theme, darkTheme) => {
        layoutConfig.darkTheme = darkTheme;
        layoutConfig.theme = theme;
    };

    const setScale = (scale) => {
        layoutConfig.scale = scale;
    };

    const setActiveMenuItem = (item) => {
        layoutConfig.activeMenuItem = item.value || item;
    };

    const onConfigButtonClick = () => {
        layoutState.configSidebarVisible = !layoutState.configSidebarVisible;
    }

    const onMenuToggle = () => {
        if (layoutConfig.menuMode === 'overlay') {
            layoutState.overlayMenuActive = !layoutState.overlayMenuActive;
        }

        if (window.innerWidth > 991) {
            layoutState.staticMenuDesktopInactive = !layoutState.staticMenuDesktopInactive;
        } else {
            layoutState.staticMenuMobileActive = !layoutState.staticMenuMobileActive;
        }
    };

    const isSidebarActive = computed(() => layoutState.overlayMenuActive || layoutState.staticMenuMobileActive);

    const isDarkTheme = computed(() => layoutConfig.darkTheme);

    const isConfigSidebarActive = computed({
        get() {
            return layoutState.configSidebarVisible
        },
        set(isVisible) {
            layoutState.configSidebarVisible = isVisible
        }
    });

    return {
        layoutConfig: toRefs(layoutConfig),
        layoutState: toRefs(layoutState),
        changeThemeSettings,
        setScale,
        onMenuToggle,
        isSidebarActive,
        isDarkTheme,
        isConfigSidebarActive,
        setActiveMenuItem,
        onConfigButtonClick
    };
}

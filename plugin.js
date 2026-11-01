(function () {
    const pluginId = "5ba28f85-65d0-4c24-b4b2-4f195826c549";

    const pluginVersion = "5.61.1.7-nevermore.2";

    window.Dashboard.getPluginPages = function () {
        return [
            {
                name: 'EmbyIcons',
                path: Dashboard.getConfigurationPageUrl('EmbyIconsConfiguration'),
                plugin: 'EmbyIcons',
                icon: 'photo'
            }
        ];
    };

    window.Dashboard.getPluginRoutes = function () {
        return [
            {
                path: '/plugins/embyiconsconfiguration.html',
                id: 'embyiconsconfiguration',

                controller: 'plugins/embyicons/embyiconsconfiguration.js?v=' + pluginVersion,

                template: 'plugins/embyicons/embyiconsconfiguration.html?v=' + pluginVersion,
                title: 'EmbyIcons',
                mobile: true
            }
        ];
    };

})();
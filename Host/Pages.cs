using MediaBrowser.Model.Plugins;
using System.Collections.Generic;
namespace EmbyIcons { public sealed partial class Plugin {
        public IEnumerable<PluginPageInfo> GetPages()
        {
            return new[]
            {
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfiguration",
                    EmbeddedResourcePath = GetType().Namespace + ".EmbyIconsConfiguration.html",
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationjs",
                    EmbeddedResourcePath = GetType().Namespace + ".EmbyIconsConfiguration.js"
                }
                ,
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationUtils",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Utils.js"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationDom",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Dom.js"
                }
                ,
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationProfile",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Profile.js"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationScans",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Scans.js"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationApi",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Api.js"
                }
                ,
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationDomCache",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.DomCache.js"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationEvents",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Events.js"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationDataLoader",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.DataLoader.js"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationUIHandlers",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.UIHandlers.js"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationProfileUI",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.ProfileUI.js"
                }
                ,
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationSettings",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Settings.html"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationIconLayout",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.IconLayout.html"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationAdvanced",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Advanced.html"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationIconManager",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.IconManager.html"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationTroubleshooter",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Troubleshooter.html"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationReadme",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.Readme.html"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationAddProfileTemplate",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.AddProfileTemplate.html"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationRenameProfileTemplate",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.RenameProfileTemplate.html"
                },
                new PluginPageInfo
                {
                    Name = "EmbyIconsConfigurationSafeZones",
                    EmbeddedResourcePath = GetType().Namespace + ".Configuration.EmbyIconsConfiguration.SafeZones.js"
                }
            };
        }

} }

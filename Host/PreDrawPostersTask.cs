using MediaBrowser.Model.Tasks;
using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;

namespace EmbyIcons.Tasks
{
    public sealed class PreDrawPostersTask : IScheduledTask
    {
        public string Name => "Pre-draw posters";
        public string Key => "EmbyIconsPreDrawPosters";
        public string Category => "EmbyIcons";
        public string Description => "Generate missing Primary poster overlays in the background, reuse existing cache files, and save completed images to the Emby disk cache.";
        public IEnumerable<TaskTriggerInfo> GetDefaultTriggers() => Array.Empty<TaskTriggerInfo>();
        public Task Execute(CancellationToken cancellationToken, IProgress<double> progress) => Plugin.Instance!.PreDraw(cancellationToken, progress);
    }
}

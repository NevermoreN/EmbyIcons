using EmbyIcons.Interop;
using MediaBrowser.Controller.Drawing;
using MediaBrowser.Controller.Entities;
using MediaBrowser.Controller.Providers;
using MediaBrowser.Model.Drawing;
using MediaBrowser.Model.Entities;
using System;
using System.Threading;
using System.Threading.Tasks;

namespace EmbyIcons
{
    public sealed class NativeEnhancer : IImageEnhancer
    {
        private static readonly AsyncLocal<Job?> Current = new AsyncLocal<Job?>();
        private sealed class Job : IDisposable
        {
            internal ulong Id, Generation; internal string Key = ""; internal bool LegacyPath; internal CancellationToken Token;
            internal Job? Previous;
            public void Dispose() => Current.Value = Previous;
        }
        internal static IDisposable Enter(ulong id, ulong generation, string key, bool legacyPath, CancellationToken token)
        { var job = new Job { Id = id, Generation = generation, Key = key, LegacyPath = legacyPath, Token = token, Previous = Current.Value }; Current.Value = job; return job; }
        private static Job? Context(BaseItem item) => Current.Value?.Id == (ulong)item.InternalId ? Current.Value : null;
        public MetadataProviderPriority Priority => MetadataProviderPriority.Last;
        public bool Supports(BaseItem item, ImageType type) => type == ImageType.Primary &&
            (Context(item) != null || (Plugin.Instance?.Core?.Query((ulong)item.InternalId).Ready ?? false));
        public string GetConfigurationCacheKey(BaseItem item, ImageType type) => Context(item)?.Key ?? Plugin.Instance!.Core!.Query((ulong)item.InternalId).Key;
        public EnhancedImageInfo GetEnhancedImageInfo(BaseItem item, string path, ImageType type, int index) =>
            new EnhancedImageInfo { RequiresTransparency = !(Context(item)?.LegacyPath ?? Plugin.Instance!.Core!.Query((ulong)item.InternalId).LegacyPath) };
        public ImageSize GetEnhancedImageSize(BaseItem item, ImageType type, int index, ImageSize size) => size;
        public Task EnhanceImageAsync(BaseItem item, string input, string output, ImageType type, int index) =>
            EnhanceImageAsync(item, input, output, type, index, CancellationToken.None);
        public Task EnhanceImageAsync(BaseItem item, string input, string output, ImageType type, int index, CancellationToken token)
        {
            var core = Plugin.Instance!.Core!;
            NativeCore.ValidateDestination(item.GetImageInfo(type, index).Path, output);
            var job = Context(item);
            if (job == null)
            {
                core.Execute(new { op = "missing", id = item.InternalId, generation = core.Query((ulong)item.InternalId).Generation });
                throw new InvalidOperationException("Poster cache is unavailable; background generation will repair it.");
            }
            job.Token.ThrowIfCancellationRequested(); token.ThrowIfCancellationRequested();
            var result = core.Execute(new { op = "render", id = job.Id, generation = job.Generation, input, output });
            if (result.TryGetProperty("legacy_miss", out _)) throw new LegacyAliasMissingException();
            job.Token.ThrowIfCancellationRequested(); token.ThrowIfCancellationRequested();
            return Task.CompletedTask;
        }
    }
    internal sealed class LegacyAliasMissingException : OperationCanceledException { }
}

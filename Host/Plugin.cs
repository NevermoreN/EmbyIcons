using EmbyIcons.Configuration;
using EmbyIcons.Interop;
using MediaBrowser.Common;
using MediaBrowser.Common.Configuration;
using MediaBrowser.Common.Plugins;
using MediaBrowser.Controller;
using MediaBrowser.Controller.Collections;
using MediaBrowser.Controller.Drawing;
using MediaBrowser.Controller.Entities;
using MediaBrowser.Controller.Entities.Movies;
using MediaBrowser.Controller.Library;
using MediaBrowser.Controller.Plugins;
using MediaBrowser.Model.Drawing;
using MediaBrowser.Model.Entities;
using MediaBrowser.Model.Logging;
using MediaBrowser.Model.Plugins;
using MediaBrowser.Model.Querying;
using MediaBrowser.Model.Serialization;
using System;
using System.Collections.Generic;
using System.Collections;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace EmbyIcons
{
    public sealed partial class Plugin : BasePlugin<PluginOptions>, IHasWebPages, IHasThumbImage, IDisposable
    {
        public static Plugin? Instance { get; private set; }
        private readonly IApplicationHost _host;
        private readonly IApplicationPaths _paths;
        private readonly ILibraryManager _library;
        private readonly IUserManager _users;
        private readonly CancellationTokenSource _stop = new CancellationTokenSource();
        private IUserDataManager _userData = null!;
        private ICollectionManager _collections = null!;
        private IImageProcessor _processor = null!;
        private Task[] _tasks = Array.Empty<Task>();
        private readonly object _manualGate = new object();
        private readonly List<Task> _manualTasks = new List<Task>();
        private SemaphoreSlim _drawSlots = null!;
        internal NativeCore? Core { get; private set; }
        internal ILogger Logger { get; }
        private int _started;
        public override string Name => "EmbyIcons (NevermoreN)";
        public override string Description => "Rust poster overlays with automatic background generation and persistent Emby cache reuse.";
        public override Guid Id => new Guid("5ba28f85-65d0-4c24-b4b2-4f195826c549");
        public ImageFormat ThumbImageFormat => ImageFormat.Png;

        public Plugin(IApplicationHost host, IApplicationPaths paths, ILibraryManager library,
            IUserManager users, ILogManager logs, IXmlSerializer serializer) : base(paths, serializer)
        { _host = host; _paths = paths; _library = library; _users = users; Logger = logs.GetLogger(nameof(Plugin)); Instance = this; }

        internal void Start()
        {
            if (Interlocked.Exchange(ref _started, 1) != 0) return;
            NativeRuntime.Load(_paths.ProgramDataPath);
            var core = new NativeCore();
            try
            {
                var paths = _host.Resolve<IServerApplicationPaths>();
                _processor = _host.Resolve<IImageProcessor>();
                _userData = _host.Resolve<IUserDataManager>();
                _collections = _host.Resolve<ICollectionManager>();
                var loaded = core.Execute(new { op = "init", config = Configuration, libraries = Libraries(),
                    data_path = Path.Combine(_paths.DataPath, "EmbyIcons"), cache_path = paths.ImageCachePath,
                    ratings_path = Path.Combine(_paths.ProgramDataPath, "plugins", "embyicons", "mdblist-ratings.json"),
                    pilot = File.Exists(Path.Combine(_paths.ProgramDataPath, "plugins", "embyicons", "rust-pilot.json")) ? JsonSerializer.Deserialize<long[]>(File.ReadAllText(Path.Combine(_paths.ProgramDataPath, "plugins", "embyicons", "rust-pilot.json"))) : Array.Empty<long>() });
                Core = core;
                _library.ItemAdded += Changed;
                _library.ItemUpdated += Changed;
                _library.ItemRemoved += Removed;
                _userData.UserDataSaved += UserChanged;
                _collections.ItemsAddedToCollection += CollectionChanged;
                _collections.ItemsRemovedFromCollection += CollectionChanged;
                var limits = core.Execute(new { op = "limits" });
                _drawSlots = new SemaphoreSlim(limits.GetProperty("render_slots").GetInt32());
                _tasks = Enumerable.Range(0, limits.GetProperty("workers").GetInt32()).Select(_ => Task.Run(Drive))
                    .Concat(new[] { Task.Run(Maintain), Task.Run(Reconcile) }).ToArray();
                Logger.Info("[EmbyIcons Rust] Started: " + loaded.GetRawText());
            }
            catch { core.Dispose(); Core = null; throw; }
        }

        private object[] Libraries() => _library.GetVirtualFolders().Select(f => (object)new
        { id = f.Id, item_id = f.ItemId, paths = f.Locations ?? Array.Empty<string>() }).ToArray();

        private BaseItem? Item(string id) => long.TryParse(id, out var number) ? _library.GetItemById(number) :
            Guid.TryParse(id, out var guid) ? _library.GetItemById(guid) : null;

        internal object Describe(BaseItem item)
        {
            var image = item.GetImageInfo(ImageType.Primary, 0);
            var members = item is BoxSet ? _library.GetItemList(new InternalItemsQuery
            { CollectionIds = new[] { item.InternalId }, IncludeItemTypes = new[] { "Movie", "Series" }, Recursive = true, Limit = 1 }) : Array.Empty<BaseItem>();
            var favorites = _users.GetUserIds(new UserQuery()).Items.Select(id => _users.GetUserById(id))
                .Where(user => user != null).Select(user => _userData.GetUserData(user, item)?.IsFavorite ?? false).ToArray();
            return new { id = item.InternalId, guid = item.Id.ToString("N"), kind = item.GetType().Name, name = item.Name,
                path = item.Path, parent_id = item.ParentId, series_id = item.SeriesId, original = image?.Path,
                image_ticks = image?.DateModified.UtcTicks, community = item.CommunityRating, critic = item.CriticRating,
                official_rating = item.OfficialRating, provider_ids = item.ProviderIds, favorite_users = favorites,
                raw_ratings = RawRatings(item), member_path = members.FirstOrDefault()?.Path };
        }

        private static object[] RawRatings(BaseItem item)
        {
            var rows = new List<object>();
            foreach (var property in item.GetType().GetProperties().Where(p => p.CanRead && p.GetIndexParameters().Length == 0 &&
                p.PropertyType != typeof(string) && typeof(IEnumerable).IsAssignableFrom(p.PropertyType) &&
                (p.Name.IndexOf("Rating", StringComparison.OrdinalIgnoreCase) >= 0 || p.Name.IndexOf("External", StringComparison.OrdinalIgnoreCase) >= 0)))
            {
                try
                {
                    if (!(property.GetValue(item) is IEnumerable values)) continue;
                    foreach (var entry in values)
                    {
                        if (entry == null) continue;
                        rows.Add(entry.GetType().GetProperties().Where(p => p.CanRead && p.GetIndexParameters().Length == 0 &&
                            new[] { "Source", "Name", "Key", "Value", "Rating", "Score" }.Contains(p.Name))
                            .ToDictionary(p => p.Name, p => Convert.ToString(p.GetValue(entry), CultureInfo.InvariantCulture)));
                    }
                }
                catch (TargetInvocationException) { /* Optional host metadata property is unavailable. */ }
            }
            return rows.ToArray();
        }

        private void Changed(object? sender, ItemChangeEventArgs e) => Forward(e.Item, false);
        private void Removed(object? sender, ItemChangeEventArgs e) => Forward(e.Item, true);
        private void Forward(BaseItem? item, bool removed, bool favoriteChange = false)
        {
            if (item == null || Core == null || _stop.IsCancellationRequested) return;
            Core.Execute(new { op = "event", id = item.InternalId, kind = item.GetType().Name,
                path = item.Path, parent_id = item.ParentId, series_id = item.SeriesId, removed, favorite_change = favoriteChange });
        }
        private void UserChanged(object? sender, UserDataSaveEventArgs e)
        { if (e.SaveReason == UserDataSaveReason.UpdateUserRating) Forward(e.Item, false, true); }
        private void CollectionChanged(object? sender, CollectionModifiedEventArgs e) => Forward(e.Collection, false);

        private async Task Drive()
        {
            var token = _stop.Token;
            while (!token.IsCancellationRequested)
            {
                ulong id = 0, generation = 0;
                try
                {
                    var next = Core!.Execute(new { op = "next" });
                    if (next.ValueKind == JsonValueKind.Null) { await Task.Delay(250, token).ConfigureAwait(false); continue; }
                    id = next.GetProperty("id").GetUInt64();
                    var item = _library.GetItemById(checked((long)id));
                    if (item == null) { Core.Execute(new { op = "remove", id }); continue; }
                    var job = Core.Execute(new { op = "observe", id, sequence = next.GetProperty("sequence").GetUInt64(), item = Describe(item) });
                    if (job.ValueKind == JsonValueKind.Null || !job.TryGetProperty("generation", out var version)) continue;
                    generation = version.GetUInt64();
                    var image = item.GetImageInfo(ImageType.Primary, 0);
                    if (image == null) throw new IOException("Original poster is not available yet.");
                    await _drawSlots.WaitAsync(token).ConfigureAwait(false);
                    try
                    {
                        using (NativeEnhancer.Enter(id, generation, job.GetProperty("key").GetString()!,
                            job.GetProperty("legacy_path").GetBoolean(), token))
                        {
                            var tag = _processor.GetImageCacheTag(item, image);
                            var output = await _processor.GetEnhancedImage(item, ImageType.Primary, 0, token).ConfigureAwait(false);
                            token.ThrowIfCancellationRequested();
                            Core.Commit(id, generation, image.Path, output, tag);
                        }
                    }
                    finally { _drawSlots.Release(); }
                }
                catch (OperationCanceledException) when (token.IsCancellationRequested)
                { if (id > 0) Core!.Execute(new { op = "cancel", id, generation }); break; }
                catch (LegacyAliasMissingException) { }
                catch (SupersededImageJobException) { /* The native core already queued the newer event. */ }
                catch (Exception ex)
                {
                    if (id > 0) Core!.Execute(new { op = "fail", id, generation, error = ex.Message });
                    Logger.Warn("[EmbyIcons Rust] Background item " + id + ": " + ex.Message);
                    await Task.Delay(250, token).ConfigureAwait(false);
                }
            }
        }

        private async Task Maintain()
        {
            var token = _stop.Token;
            while (!token.IsCancellationRequested)
            {
                try { await Task.Delay(10000, token).ConfigureAwait(false); Core!.Execute(new { op = "maintenance" }); Core.Execute(new { op = "save" }); }
                catch (OperationCanceledException) when (token.IsCancellationRequested) { break; }
                catch (Exception ex) { Logger.Warn("[EmbyIcons Rust] Maintenance: " + ex.Message); }
            }
        }

        private async Task Reconcile()
        {
            var token = _stop.Token;
            while (!token.IsCancellationRequested)
            {
                try
                {
                    if (Core!.Execute(new { op = "reconcile", ack = true }).GetProperty("needed").GetBoolean())
                        await Scan(null, null, token).ConfigureAwait(false);
                    await Task.Delay(10000, token).ConfigureAwait(false);
                }
                catch (OperationCanceledException) when (token.IsCancellationRequested) { break; }
                catch (Exception ex) { Logger.Warn("[EmbyIcons Rust] Library reconciliation: " + ex.Message); await Task.Delay(10000, token).ConfigureAwait(false); }
            }
        }

        private IEnumerable<long[]> ScanPages()
        {
            var scopes = Core!.Execute(new { op = "scopes" });
            var pilot = scopes.GetProperty("pilot").EnumerateArray().Select(v => v.GetInt64()).Where(id => id > 0).ToArray();
            if (pilot.Length > 0) { yield return pilot; yield break; }
            // Favorites apply server-wide, including libraries without a profile.
            var favorites = new HashSet<long>();
            foreach (var userId in _users.GetUserIds(new UserQuery()).Items)
            {
                var user = _users.GetUserById(userId);
                if (user == null) continue;
                favorites.UnionWith(_library.GetInternalItemIds(new InternalItemsQuery(user) { Recursive = true,
                    IsFavorite = true, IncludeItemTypes = new[] { "Movie", "Series", "BoxSet" }, IsVirtualItem = false }));
            }
            var favoriteIds = favorites.ToArray();
            for (int i = 0; i < favoriteIds.Length; i += 128) yield return favoriteIds.Skip(i).Take(128).ToArray();
            var ids = scopes.GetProperty("libraries").EnumerateArray().Select(v => v.GetString()).ToArray();
            foreach (var folder in _library.GetVirtualFolders().Where(f => ids.Contains(f.Id)))
            {
                var parent = Item(folder.ItemId);
                if (parent == null) { Logger.Warn("[EmbyIcons Rust] Skipped unresolved mapped library " + folder.Id); continue; }
                var snapshot = _library.GetInternalItemIds(new InternalItemsQuery { AncestorIds = new[] { parent.InternalId }, Recursive = true,
                    IncludeItemTypes = new[] { "Movie", "Series" }, IsVirtualItem = false }).Where(id => !favorites.Contains(id)).ToArray();
                for (int i = 0; i < snapshot.Length; i += 128) yield return snapshot.Skip(i).Take(128).ToArray();
            }
            if (scopes.GetProperty("collections").GetBoolean())
            {
                var snapshot = _library.GetInternalItemIds(new InternalItemsQuery { Recursive = true, IncludeItemTypes = new[] { "BoxSet" }, IsVirtualItem = false })
                    .Where(id => !favorites.Contains(id)).ToArray();
                for (int i = 0; i < snapshot.Length; i += 128) yield return snapshot.Skip(i).Take(128).ToArray();
            }
        }

        private async Task Scan(string? batch, IProgress<double>? progress, CancellationToken token)
        {
            // Snapshot lightweight IDs for an accurate scan denominator. Full
            // item metadata is loaded once, by the worker that observes it.
            var pages = ScanPages().ToArray();
            var total = pages.Sum(page => page.Length);
            var scanned = 0;
            foreach (var page in pages)
            {
                token.ThrowIfCancellationRequested();
                while (batch == null && Core!.Execute(new { op = "capacity" }).GetProperty("backpressure").GetBoolean())
                    await Task.Delay(500, token).ConfigureAwait(false);
                Core!.Execute(new { op = "notify", ids = page, batch, automatic = batch == null });
                scanned += page.Length;
                progress?.Report(total == 0 ? 50 : 1 + 49.0 * scanned / total);
            }
            progress?.Report(50);
        }

        internal Task PreDraw(CancellationToken token, IProgress<double> progress)
        {
            lock (_manualGate)
            {
                _stop.Token.ThrowIfCancellationRequested();
                progress.Report(1);
                var task = Task.Run(() => RunPreDraw(token, progress));
                _manualTasks.Add(task);
                _ = task.ContinueWith(done => { lock (_manualGate) _manualTasks.Remove(done); }, TaskScheduler.Default);
                return task;
            }
        }

        private async Task RunPreDraw(CancellationToken requested, IProgress<double> progress)
        {
            using var linked = CancellationTokenSource.CreateLinkedTokenSource(requested, _stop.Token);
            var token = linked.Token;
            var batch = Guid.NewGuid().ToString("N");
            try
            {
                await Scan(batch, progress, token).ConfigureAwait(false);
                JsonElement status;
                do
                {
                    token.ThrowIfCancellationRequested(); status = Core!.Execute(new { op = "batch_status", batch });
                    var total = status.GetProperty("total").GetInt32(); var pending = status.GetProperty("pending").GetInt32();
                    progress.Report(total == 0 ? 100 : 50 + 50.0 * (total - pending) / total);
                    if (status.GetProperty("finished").GetBoolean()) break;
                    await Task.Delay(500, token).ConfigureAwait(false);
                } while (true);
                Core!.Execute(new { op = "save" });
                var final = Core.Execute(new { op = "batch_finish", batch });
                Logger.Info("[EmbyIcons Rust] Manual pre-draw result: " + final.GetRawText()); progress.Report(100);
            }
            finally { Core!.Execute(new { op = "batch_end", batch, cancel = token.IsCancellationRequested }); }
        }

        public override void UpdateConfiguration(BasePluginConfiguration value)
        {
            var options = (PluginOptions)value;
            options.PersistedVersion = Configuration.PersistedVersion;
            Core!.Execute(new { op = "validate_config", config = options });
            base.UpdateConfiguration(options);
            Core.Execute(new { op = "configure", config = options, libraries = Libraries() });
        }

        public Stream GetThumbImage() => typeof(Plugin).Assembly.GetManifestResourceStream("EmbyIcons.Images.logo.png")!;
        public void Dispose()
        {
            if (Core == null) return;
            _library.ItemAdded -= Changed; _library.ItemUpdated -= Changed; _library.ItemRemoved -= Removed;
            _userData.UserDataSaved -= UserChanged;
            _collections.ItemsAddedToCollection -= CollectionChanged; _collections.ItemsRemovedFromCollection -= CollectionChanged;
            _stop.Cancel();
            Task[] pending;
            lock (_manualGate) pending = _tasks.Concat(_manualTasks).ToArray();
            try { Task.WhenAll(pending).GetAwaiter().GetResult(); }
            catch (OperationCanceledException) { }
            try { Core.Execute(new { op = "save" }); }
            finally { Core.Dispose(); Core = null; _drawSlots.Dispose(); _stop.Dispose(); }
        }
    }

    public sealed class NativeEntryPoint : IServerEntryPoint
    { public void Run() => Plugin.Instance!.Start(); public void Dispose() { } }
}

using EmbyIcons.Api;
using EmbyIcons.Configuration;
using MediaBrowser.Controller.Net;
using MediaBrowser.Model.Services;
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace EmbyIcons
{
    [Authenticated(Roles = "Admin"), Route("/EmbyIcons/Runtime", "GET,POST")]
    public class RuntimeRequest { public long[]? ItemIds { get; set; } public bool ReleasePilot { get; set; } }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.DefaultProfile, "GET")]
    public class DefaultRequest { }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.GetApiRoutes, "GET")]
    public class RoutesRequest { }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.RefreshCache, "POST")]
    public class RefreshRequest { }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.ExportProfiles, "GET")]
    public class ExportRequest { public string? ProfileIds { get; set; } public bool IncludeLibraryMappings { get; set; } }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.ImportProfiles, "POST")]
    public class ImportRequest { public string JsonData { get; set; } = ""; public bool OverwriteExisting { get; set; } public bool RenameOnConflict { get; set; } = true; public bool ImportLibraryMappings { get; set; } = true; }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.ValidateProfileImport, "POST")]
    public class ValidateImportRequest { public string JsonData { get; set; } = ""; }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.Preview, "GET")]
    public class PreviewRequest { public string OptionsJson { get; set; } = ""; }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.IconManagerReport, "GET")]
    public class IconsRequest { }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.ItemsByIcon, "GET")]
    public class ItemsRequest { public string IconType { get; set; } = ""; public string IconName { get; set; } = ""; public int Limit { get; set; } = 100; }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.MovieTroubleshooter, "GET")]
    public class MoviesRequest { public string? MovieId { get; set; } public string? ChecksToRun { get; set; } }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.SeriesTroubleshooter, "GET")]
    public class SeriesRequest { public string? SeriesId { get; set; } public string? ChecksToRun { get; set; } }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.ScanProgress, "GET")]
    public class ProgressRequest { public string? ScanType { get; set; } }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.TemplateCacheStats, "GET")]
    public class TemplatesRequest { }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.ClearTemplateCache, "POST")]
    public class ClearTemplatesRequest { }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.MemoryUsage, "GET")]
    public class MemoryRequest { }
    [Authenticated(Roles = "Admin"), Route(ApiRoutes.StaticImage, "GET")]
    public class StaticRequest { public string Name { get; set; } = ""; }

    public sealed class NativeServices : IService
    {
        private static Plugin Plugin => EmbyIcons.Plugin.Instance!;
        private static object Run(object command) => Plain(Plugin.Core!.Execute(command))!;
        internal static object? Plain(JsonElement value)
        {
            switch (value.ValueKind)
            {
                case JsonValueKind.Object: return value.EnumerateObject().ToDictionary(p => p.Name, p => Plain(p.Value));
                case JsonValueKind.Array: return value.EnumerateArray().Select(Plain).ToArray();
                case JsonValueKind.String: return value.GetString();
                case JsonValueKind.Number: return value.TryGetInt64(out var n) ? (object)n : value.GetDouble();
                case JsonValueKind.True: return true;
                case JsonValueKind.False: return false;
                default: return null;
            }
        }
        public object Get(RuntimeRequest request) => request.ItemIds == null ? Run(new { op = "stats" }) :
            new { Stats = Run(new { op = "stats" }), Items = request.ItemIds.Select(id => Run(new { op = "inspect", id })).ToArray() };
        public object Post(RuntimeRequest request)
        {
            if (request.ReleasePilot) Plugin.Core!.Execute(new { op = "pilot", ids = Array.Empty<long>() });
            if (request.ItemIds != null) Plugin.Core!.Execute(new { op = "notify", ids = request.ItemIds, automatic = true });
            return Get(request);
        }
        public object Get(DefaultRequest request) => Run(new { op = "manage", action = "default_profile", template = new IconProfile() });
        public object Get(RoutesRequest request) => typeof(ApiRoutes).GetFields(BindingFlags.Public | BindingFlags.Static)
            .Where(f => !new[] { "MusicTroubleshooter", "AspectRatio", "ValidatePath" }.Contains(f.Name)).ToDictionary(f => f.Name, f => f.GetRawConstantValue());
        public object Post(RefreshRequest request) => Run(new { op = "manage", action = "refresh" });
        public object Get(ExportRequest request) => Run(new { op = "manage", action = "export", ids = request.ProfileIds, mappings = request.IncludeLibraryMappings });
        public object Post(ValidateImportRequest request) => Run(new { op = "manage", action = "validate_import", json = request.JsonData });
        public object Post(ImportRequest request)
        {
            var result = Plugin.Core!.Execute(new { op = "manage", action = "import", json = request.JsonData,
                overwrite = request.OverwriteExisting, rename = request.RenameOnConflict, mappings = request.ImportLibraryMappings });
            var jsonOptions = new JsonSerializerOptions(); jsonOptions.Converters.Add(new JsonStringEnumConverter());
            var options = JsonSerializer.Deserialize<PluginOptions>(result.GetProperty("config").GetRawText(), jsonOptions)!;
            Plugin.UpdateConfiguration(options);
            return Plain(result.GetProperty("response"))!;
        }
        public object Get(PreviewRequest request)
        {
            if (request.OptionsJson.Length > 256 * 1024) throw new ArgumentException("Preview settings are too large.");
            var options = new JsonSerializerOptions { PropertyNameCaseInsensitive = true };
            options.Converters.Add(new JsonStringEnumConverter());
            var settings = JsonSerializer.Deserialize<ProfileSettings>(request.OptionsJson, options)!;
            var result = Plugin.Core!.Execute(new { op = "preview", settings });
            var path = result.GetProperty("path").GetString()!;
            return new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 65536, FileOptions.DeleteOnClose);
        }
        public object Get(IconsRequest request) => Run(new { op = "manage", action = "icon_report" });
        public object Get(ItemsRequest request) => Run(new { op = "manage", action = "items", icon = request.IconName, limit = request.Limit });
        public object Get(MoviesRequest request) => Run(new { op = "manage", action = "diagnostic", kind = "Movie", id = request.MovieId });
        public object Get(SeriesRequest request) => Run(new { op = "manage", action = "diagnostic", kind = "Series", id = request.SeriesId });
        public object Get(ProgressRequest request) => Run(new { op = "manage", action = "progress" });
        public object Get(TemplatesRequest request) => Run(new { op = "manage", action = "template_stats" });
        public object Post(ClearTemplatesRequest request) => Run(new { op = "manage", action = "clear_templates" });
        public object Get(MemoryRequest request)
        {
            using var process = Process.GetCurrentProcess();
            return new { ProcessWorkingSetBytes = process.WorkingSet64, ProcessPrivateBytes = process.PrivateMemorySize64,
                ManagedHeapBytes = GC.GetTotalMemory(false), IconCacheEstimatedBytes = 0, SeriesAggregationCacheCount = 0,
                EpisodeCacheCount = 0, TimestampUtc = DateTime.UtcNow.ToString("o"), Native = Run(new { op = "stats" }) };
        }
        public object Get(StaticRequest request)
        {
            var name = Path.GetFileNameWithoutExtension(request.Name);
            if (!new[] { "logo", "preview", "tag" }.Contains(name)) throw new ArgumentException("Unknown image asset.");
            return typeof(Plugin).Assembly.GetManifestResourceStream("EmbyIcons.Images." + name + ".png")!;
        }
    }
}

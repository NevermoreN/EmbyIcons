using Emby.Api.Images;
using Emby.Drawing;
using Emby.Server.Implementations.Cryptography;
using EmbyIcons.Interop;
using MediaBrowser.Common.Extensions;
using MediaBrowser.Controller;
using MediaBrowser.Controller.Drawing;
using MediaBrowser.Controller.Entities;
using MediaBrowser.Controller.Entities.Movies;
using MediaBrowser.Controller.IO;
using MediaBrowser.Controller.Library;
using MediaBrowser.Controller.Net;
using MediaBrowser.Controller.Persistence;
using MediaBrowser.Controller.Providers;
using MediaBrowser.Model.Drawing;
using MediaBrowser.Model.Entities;
using MediaBrowser.Model.IO;
using MediaBrowser.Model.Logging;
using MediaBrowser.Model.Serialization;
using MediaBrowser.Model.Services;
using Moq;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

// Actual host processor + image service, real Rust core and synthetic image payloads.
// The codec and result factory are substitutes: this checks cache flow, not pixels
// or actual HTTP response headers. No production server state is accessed.
var root = Path.Combine(Path.GetTempPath(), "embyicons-host-probe-" + Guid.NewGuid().ToString("N"));
Directory.CreateDirectory(root);
BaseExtensions.CryptographyProvider = new CryptographyProvider();
try
{
    if (args.Length == 2 && args[0] == "--legacy-fixtures")
    {
        using var migrationCore = new NativeCore();
        using var fixtures = JsonDocument.Parse(File.ReadAllText(args[1]));
        var migrationPaths = new Mock<IServerApplicationPaths>();
        migrationPaths.SetupGet(p => p.ImageCachePath).Returns(root);
        var enhancer = new ProbeEnhancer(migrationCore);
        using var migrationProcessor = new ImageProcessor(Mock.Of<ILogger>(), migrationPaths.Object, Mock.Of<IFileSystem>(),
            Mock.Of<IJsonSerializer>(), () => Mock.Of<ILibraryManager>());
        migrationProcessor.AddParts(new[] { enhancer });
        var tags = new List<object>();
        foreach (var fixture in fixtures.RootElement.EnumerateArray())
        {
            BaseItem migrationItem = fixture.GetProperty("kind").GetString() == "Series"
                ? new MediaBrowser.Controller.Entities.TV.Series() : new Movie();
            migrationItem.InternalId = fixture.GetProperty("id").GetInt64();
            var migrationImage = new ItemImageInfo { Path = fixture.GetProperty("original").GetString(), Type = ImageType.Primary,
                DateModified = new DateTimeOffset(fixture.GetProperty("ticks").GetInt64(), TimeSpan.Zero) };
            migrationItem.ImageInfos = new[] { migrationImage };
            enhancer.JobKey = fixture.GetProperty("key").GetString();
            var tag = migrationProcessor.GetImageCacheTag(migrationItem, migrationImage);
            tags.Add(new { migrationItem.InternalId, matches = tag == fixture.GetProperty("expectedTag").GetString(), tag });
        }
        Console.WriteLine(JsonSerializer.Serialize(tags));
        return;
    }
    using var core = new NativeCore();
    var hashInput = "海报🎬|UTF-8|é";
    var nativeHash = core.Execute(new { op = "hash", text = hashInput }).GetString();
    Check(nativeHash == Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(hashInput)))[..32], "legacy SHA-256 fingerprint encoding");
    core.Execute(new { op = "stage", id = 123UL, legacy_key = "probe-stable-key", fingerprint = "visual-1" });
    var paths = new Mock<IServerApplicationPaths>();
    paths.SetupGet(p => p.ProgramDataPath).Returns(root);
    paths.SetupGet(p => p.InternalMetadataPath).Returns(root);
    paths.SetupGet(p => p.ImageCachePath).Returns(Path.Combine(root, "images"));
    paths.SetupGet(p => p.TempDirectory).Returns(Path.Combine(root, "temp"));
    Directory.CreateDirectory(Path.Combine(root, "temp"));
    var files = new Mock<IFileSystem>();
    files.Setup(f => f.FileExists(It.IsAny<string>())).Returns((string p) => File.Exists(p));
    files.Setup(f => f.GetDirectoryName(It.IsAny<string>())).Returns((string p) => Path.GetDirectoryName(p)!);
    files.Setup(f => f.CreateDirectory(It.IsAny<string>())).Callback((string p) => Directory.CreateDirectory(p));
    files.Setup(f => f.GetLastWriteTimeUtc(It.IsAny<string>())).Returns((string p) => File.GetLastWriteTimeUtc(p));
    files.Setup(f => f.GetLastWriteTimeUtc(It.IsAny<string>(), It.IsAny<bool>())).Returns((string p, bool _) => File.GetLastWriteTimeUtc(p));
    files.Setup(f => f.MoveFile(It.IsAny<string>(), It.IsAny<string>(), true)).Callback((string a, string b, bool _) => File.Move(a, b, true));
    files.Setup(f => f.ContainsSubPath(It.IsAny<string>(), It.IsAny<string>())).Returns(true);
    var source = Path.Combine(root, "source.jpg");
    File.WriteAllBytes(source, new byte[] { 1, 2, 3 });
    bool originalRejected = false;
    try { NativeCore.ValidateDestination(source, Path.Combine(root, ".", "source.jpg")); }
    catch (IOException) { originalRejected = true; }
    Check(originalRejected, "original artwork cannot be used as the output destination");
    var image = new ItemImageInfo { Path = source, Type = ImageType.Primary, DateModified = DateTimeOffset.UtcNow, Width = 600, Height = 900 };
    var item = new Movie { InternalId = 123, Name = "probe", ImageInfos = new[] { image } };
    var draw = new ProbeEnhancer(core);
    using var processor = new ImageProcessor(Mock.Of<ILogger>(), paths.Object, files.Object,
        Mock.Of<IJsonSerializer>(), () => Mock.Of<ILibraryManager>());
    processor.AddParts(new[] { draw });
    var codec = new Mock<IImageEncoder>();
    codec.SetupGet(c => c.Name).Returns("probe-codec");
    codec.SetupGet(c => c.SupportsImageEncoding).Returns(true);
    codec.SetupGet(c => c.SupportedInputFormats).Returns(new[] { "jpg", "png" });
    codec.SetupGet(c => c.SupportedOutputFormats).Returns(new[] { ImageFormat.Jpg, ImageFormat.Png });
    codec.Setup(c => c.EncodeImage(It.IsAny<string>(), It.IsAny<string>(), It.IsAny<bool>(), It.IsAny<ImageOrientation?>(), It.IsAny<int>(), It.IsAny<ImageProcessingOptions>(), It.IsAny<ImageFormat>()))
        .Returns((string input, string output, bool orient, ImageOrientation? orientation, int quality, ImageProcessingOptions options, ImageFormat format) =>
        {
            File.Copy(input, output, true);
            return output;
        });
    processor.SetImageEncoders(new List<IImageEncoder> { codec.Object });
    var auth = new Mock<IAuthorizationContext>();
    auth.Setup(a => a.GetAuthorizationInfo(It.IsAny<IRequest>())).Returns(new AuthorizationInfo { Token = "synthetic-probe" });
    var results = new Mock<IHttpResultFactory>();
    results.Setup(r => r.GetStaticFileResult(It.IsAny<IRequest>(), It.IsAny<StaticFileResultOptions>()))
        .Returns((IRequest r, StaticFileResultOptions o) => Task.FromResult<object>(o));
    var service = new ImageService(Mock.Of<IIOManager>(), Mock.Of<IProviderManager>(), Mock.Of<IItemRepository>(), processor, files.Object)
    { Request = Mock.Of<IRequest>(), AuthorizationContext = auth.Object, ResultFactory = results.Object };

    async Task<StaticFileResultOptions> Request(string tag, string format) =>
        (StaticFileResultOptions)await service.GetImage(new GetItemImage
        { Id = "123", Type = ImageType.Primary, Tag = tag, Format = format, EnableImageEnhancers = true, MaxWidth = 240 }, item.InternalId, item, false);

    async Task<string> Predraw()
    {
        var job = core.Execute(new { op = "begin", id = 123UL });
        Check(job.ValueKind != JsonValueKind.Null, "job acquired");
        var generation = job.GetProperty("generation").GetUInt64();
        draw.JobKey = job.GetProperty("key").GetString();
        try
        {
            var expectedTag = processor.GetImageCacheTag(item, image);
            var path = await processor.GetEnhancedImage(item, ImageType.Primary, 0, CancellationToken.None);
            core.Commit(123, generation, source, path, expectedTag);
            return path;
        }
        finally { draw.JobKey = null; }
    }

    var originalTag = processor.GetImageCacheTag(item, image);
    var notReady = await Request(originalTag, "Jpg");
    Check(draw.DrawCount == 0 && File.ReadAllBytes(notReady.Path).SequenceEqual(new byte[] { 1, 2, 3 }), "not-ready request only returns original pixels");
    var generated = await Predraw();
    var publishedTag = processor.GetImageCacheTag(item, image);
    Check(publishedTag != originalTag && Path.GetFileNameWithoutExtension(generated) == publishedTag, "published tag matches actual host output");
    var mtime = File.GetLastWriteTimeUtc(generated);
    var hit = await Request(publishedTag, "Png");
    Check(draw.DrawCount == 1 && File.GetLastWriteTimeUtc(generated) == mtime && File.ReadAllBytes(hit.Path).SequenceEqual(new byte[] { 4, 5, 6 }), "ready request reuses enhanced output");

    File.Delete(generated);
    File.Delete(hit.Path); // force the actual host cache-miss fallback path
    var missing = await Request(publishedTag, "Png");
    Check(draw.DrawCount == 1 && !core.Query(123).Ready && !File.Exists(generated), "missing-cache request never draws or writes a false enhanced file");
    Check(File.ReadAllBytes(missing.Path).SequenceEqual(new byte[] { 1, 2, 3 }) && missing.CacheDuration == TimeSpan.FromDays(365), "tagged original fallback has year-long cache policy");
    var repaired = await Predraw();
    var repairedTag = processor.GetImageCacheTag(item, image);
    var repairedResponse = await Request(repairedTag, "Png");
    Check(repairedTag != publishedTag && repaired != generated && repairedResponse.Path != missing.Path,
        "repaired image uses a fresh URL and cannot hit the cached original fallback");
    Check(draw.DrawCount == 2 && File.ReadAllBytes(repairedResponse.Path).SequenceEqual(new byte[] { 4, 5, 6 }), "only background initial/recovery jobs draw");
    Check(File.ReadAllBytes(source).SequenceEqual(new byte[] { 1, 2, 3 }), "original artwork remains untouched");
    var final = core.Query(123);
    core.Execute(new { op = "stage", id = 123UL, legacy_key = "probe-stable-key", fingerprint = "visual-2" });
    var stale = core.Execute(new { op = "begin", id = 123UL }).GetProperty("generation").GetUInt64();
    core.Execute(new { op = "stage", id = 123UL, legacy_key = "probe-stable-key", fingerprint = "visual-3" });
    var staleCommit = core.Execute(new { op = "commit", id = 123UL, generation = stale, path = repaired });
    Check(!staleCommit.GetProperty("published").GetBoolean() && !core.Query(123).Ready && core.Query(123).Key == final.Key, "superseded job cannot publish a new version");
    Console.WriteLine(JsonSerializer.Serialize(new
    {
        hostSha256 = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(typeof(ImageProcessor).Assembly.Location))),
        checks = "native ABI/hash/publication/not-ready/background/hit/HTTP-fallback/recovery/stale-commit passed",
        actualDraws = draw.DrawCount,
        taggedFallbackCacheDays = missing.CacheDuration?.TotalDays,
        syntheticCodec = true,
        actualHttpHeadersVerified = false
    }));
}
finally { Directory.Delete(root, recursive: true); }

static void Check(bool value, string message)
{
    if (!value) throw new InvalidOperationException(message);
}

sealed class ProbeEnhancer(NativeCore core) : IImageEnhancer
{
    public string? JobKey;
    public int DrawCount;
    public MetadataProviderPriority Priority => MetadataProviderPriority.Last;
    public bool Supports(BaseItem item, ImageType type) => type == ImageType.Primary && (JobKey != null || core.Query((ulong)item.InternalId).Ready);
    public string GetConfigurationCacheKey(BaseItem item, ImageType type) => JobKey ?? core.Query((ulong)item.InternalId).Key;
    public EnhancedImageInfo GetEnhancedImageInfo(BaseItem item, string path, ImageType type, int index) => new() { RequiresTransparency = true };
    public ImageSize GetEnhancedImageSize(BaseItem item, ImageType type, int index, ImageSize size) => size;
    public Task EnhanceImageAsync(BaseItem item, string input, string output, ImageType type, int index)
    {
        NativeCore.ValidateDestination(item.GetImageInfo(type, index).Path, output);
        if (JobKey == null)
        {
            core.Execute(new { op = "missing", id = (ulong)item.InternalId, generation = core.Query((ulong)item.InternalId).Generation });
            throw new InvalidOperationException("Background generation required.");
        }
        DrawCount++;
        File.WriteAllBytes(output, new byte[] { 4, 5, 6 });
        return Task.CompletedTask;
    }
}

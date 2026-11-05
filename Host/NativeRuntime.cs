using EmbyIcons.Interop;
using System;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

namespace EmbyIcons
{
    internal static class NativeRuntime
    {
        internal static void Load(string programDataPath)
        {
            if (!RuntimeInformation.IsOSPlatform(OSPlatform.Linux) || RuntimeInformation.ProcessArchitecture != Architecture.X64)
                throw new PlatformNotSupportedException("This EmbyIcons package requires Linux x86_64.");
            using var bundle = typeof(NativeRuntime).Assembly.GetManifestResourceStream("EmbyIcons.Native.linux-x64.zip")
                ?? throw new InvalidOperationException("The EmbyIcons native runtime bundle is missing.");
            using var sha = SHA256.Create();
            var revision = BitConverter.ToString(sha.ComputeHash(bundle)).Replace("-", "").ToLowerInvariant();
            var folder = Path.Combine(programDataPath, "plugins", "embyicons", "native", revision);
            Directory.CreateDirectory(folder);
            var boundary = Path.GetFullPath(folder) + Path.DirectorySeparatorChar;
            bundle.Position = 0;
            using var archive = new ZipArchive(bundle, ZipArchiveMode.Read);
            foreach (var entry in archive.Entries)
            {
                if (entry.FullName.EndsWith("/", StringComparison.Ordinal)) continue;
                var destination = Path.GetFullPath(Path.Combine(folder, entry.FullName));
                if (!destination.StartsWith(boundary, StringComparison.Ordinal))
                    throw new InvalidDataException("Native bundle entry is outside its plugin directory.");
                using var input = entry.Open();
                var expected = sha.ComputeHash(input);
                if (File.Exists(destination))
                {
                    using var existing = File.OpenRead(destination);
                    if (existing.Length == entry.Length && sha.ComputeHash(existing).SequenceEqual(expected)) continue;
                }
                Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
                var temporary = destination + "." + Guid.NewGuid().ToString("N") + ".tmp";
                try
                {
                    using (var source = entry.Open())
                    using (var output = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                    { source.CopyTo(output); output.Flush(true); }
                    if (File.Exists(destination)) File.Replace(temporary, destination, null);
                    else File.Move(temporary, destination);
                }
                finally { if (File.Exists(temporary)) File.Delete(temporary); }
            }
            NativeCore.Load(Path.Combine(folder, "libembyicons_core.so"));
        }
    }
}

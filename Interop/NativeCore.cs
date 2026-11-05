using Microsoft.Win32.SafeHandles;
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Reflection;

namespace EmbyIcons.Interop
{
    internal sealed class NativeCore : IDisposable
    {
        private const uint AbiVersion = 2;
        private readonly CoreHandle _handle;
        private static IntPtr _library;

        internal static void Load(string path)
        {
            // NativeLibrary is provided by the actual .NET 8 host. Reflection keeps
            // this mechanical adapter compatible with its netstandard2.0 contract.
            var type = Type.GetType("System.Runtime.InteropServices.NativeLibrary, System.Private.CoreLib", true)!;
            _library = (IntPtr)type.GetMethod("Load", new[] { typeof(string) })!.Invoke(null, new object[] { path })!;
            var set = type.GetMethod("SetDllImportResolver")!;
            var resolver = Delegate.CreateDelegate(set.GetParameters()[1].ParameterType,
                typeof(NativeCore).GetMethod(nameof(Resolve), BindingFlags.NonPublic | BindingFlags.Static)!);
            set.Invoke(null, new object[] { typeof(NativeCore).Assembly, resolver });
        }

        private static IntPtr Resolve(string name, Assembly assembly, DllImportSearchPath? path) =>
            name == "embyicons_core" ? _library : IntPtr.Zero;

        internal NativeCore()
        {
            if (NativeMethods.AbiVersion() != AbiVersion)
                throw new InvalidOperationException("Unsupported EmbyIcons native ABI.");
            _handle = new CoreHandle(NativeMethods.Create());
            if (_handle.IsInvalid) throw new InvalidOperationException("Could not create the native core.");
        }

        internal JsonElement Execute(object command)
        {
            var input = JsonSerializer.SerializeToUtf8Bytes(command, command.GetType());
            var status = NativeMethods.Execute(_handle, input, (UIntPtr)input.Length, out var output);
            if (status != 0) throw new InvalidOperationException("Native command failed: " + status);
            try
            {
                var bytes = new byte[checked((int)output.Length.ToUInt64())];
                Marshal.Copy(output.Data, bytes, 0, bytes.Length);
                using var reply = JsonDocument.Parse(bytes);
                var value = reply.RootElement.GetProperty("value");
                if (!reply.RootElement.GetProperty("ok").GetBoolean())
                    throw new InvalidOperationException(value.GetString());
                return value.Clone();
            }
            finally { NativeMethods.Free(output); }
        }

        internal unsafe PublishedImage Query(ulong id)
        {
            var status = NativeMethods.Query(_handle, id, out var result);
            if (status != 0) throw new InvalidOperationException("Native query failed: " + status);
            if (result.KeyLength > 512) throw new InvalidOperationException("Invalid native key length.");
            return new PublishedImage((result.Ready & 1) != 0, (result.Ready & 2) != 0, result.Generation,
                Encoding.UTF8.GetString(result.Key, (int)result.KeyLength));
        }

        internal static void ValidateDestination(string originalFile, string outputFile)
        {
            if (string.Equals(Path.GetFullPath(originalFile), Path.GetFullPath(outputFile), StringComparison.Ordinal))
                throw new IOException("EmbyIcons must not replace original artwork.");
        }

        internal void Commit(ulong id, ulong generation, string originalFile, string outputFile, string expectedHostTag)
        {
            ValidateDestination(originalFile, outputFile);
            // Background completion only: a successful callback alone is insufficient.
            if (!File.Exists(outputFile) || !string.Equals(Path.GetFileNameWithoutExtension(outputFile), expectedHostTag, StringComparison.Ordinal))
                throw new IOException("The host did not return the expected enhanced output.");
            var result = Execute(new { op = "commit", id, generation, path = outputFile });
            if (!result.GetProperty("published").GetBoolean())
                throw new OperationCanceledException("The image job was superseded.");
        }

        public void Dispose() => _handle.Dispose();

        internal readonly struct PublishedImage
        {
            internal readonly bool Ready;
            internal readonly bool LegacyPath;
            internal readonly ulong Generation;
            internal readonly string Key;
            internal PublishedImage(bool ready, bool legacyPath, ulong generation, string key)
            { Ready = ready; LegacyPath = legacyPath; Generation = generation; Key = key; }
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct NativeBuffer { internal IntPtr Data; internal UIntPtr Length; }

        [StructLayout(LayoutKind.Sequential)]
        private unsafe struct NativeQuery
        {
            internal ulong Generation;
            internal uint Ready;
            internal uint KeyLength;
            internal fixed byte Key[512];
        }

        private sealed class CoreHandle : SafeHandleZeroOrMinusOneIsInvalid
        {
            internal CoreHandle(IntPtr value) : base(true) { SetHandle(value); }
            protected override bool ReleaseHandle() { NativeMethods.Close(handle); return true; }
        }

        private static class NativeMethods
        {
            private const string Library = "embyicons_core";
            [DllImport(Library, EntryPoint = "ei_abi_version", CallingConvention = CallingConvention.Cdecl)]
            internal static extern uint AbiVersion();
            [DllImport(Library, EntryPoint = "ei_create", CallingConvention = CallingConvention.Cdecl)]
            internal static extern IntPtr Create();
            [DllImport(Library, EntryPoint = "ei_close", CallingConvention = CallingConvention.Cdecl)]
            internal static extern void Close(IntPtr context);
            [DllImport(Library, EntryPoint = "ei_execute", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int Execute(CoreHandle context, byte[] data, UIntPtr length, out NativeBuffer output);
            [DllImport(Library, EntryPoint = "ei_buffer_free", CallingConvention = CallingConvention.Cdecl)]
            internal static extern void Free(NativeBuffer buffer);
            [DllImport(Library, EntryPoint = "ei_query", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int Query(CoreHandle context, ulong id, out NativeQuery output);
        }
    }
}

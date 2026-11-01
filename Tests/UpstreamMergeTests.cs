using EmbyIcons.Configuration;
using EmbyIcons.Helpers;
using EmbyIcons.Services;
using MediaBrowser.Controller.Net;
using MediaBrowser.Model.Logging;
using Moq;
using System;
using System.Collections.Generic;
using System.Reflection;
using System.Threading.Tasks;
using Xunit;

namespace EmbyIcons.Tests
{
    public class UpstreamMergeTests
    {
        [Theory]
        [InlineData("/media/movies", "/media/movies/example.mkv", true)]
        [InlineData("/media/movies", "/media/movies-old/example.mkv", false)]
        [InlineData("/media/movies/", "/media/movies", true)]
        [InlineData("C:\\Media\\Movies\\", "c:/media/movies/example.mkv", true)]
        [InlineData("C:\\Media\\Movies", "c:\\media\\movies-old\\example.mkv", false)]
        public void LibraryPathsRespectDirectoryBoundaries(string library, string item, bool matches)
        {
            var trie = new Trie<string>();
            trie.Insert(library, "configured-profile");
            Assert.Equal(matches ? "configured-profile" : null, trie.FindLongestPrefix(item));
        }

        [Fact]
        public void ImportRestoresValidMappingsAndSanitizesNullSettings()
        {
            var options = new PluginOptions();
            var service = new ProfileImportExportService(Mock.Of<ILogger>(), options);
            const string json = "{\"Profiles\":[{\"Name\":\"Restored\",\"Settings\":{\"FilenameBasedIcons\":null,\"TagBasedIcons\":null}}],\"LibraryMappings\":[{\"LibraryId\":\"existing\",\"ProfileName\":\"Restored\"},{\"LibraryId\":\"missing\",\"ProfileName\":\"Restored\"}]}";

            var result = service.ImportProfilesFromJson(json, new ImportOptions
            {
                ImportLibraryMappings = true,
                KnownLibraryIds = new HashSet<string> { "existing" }
            });

            Assert.True(result.Success);
            var profile = Assert.Single(options.Profiles);
            Assert.NotNull(profile.Settings.FilenameBasedIcons);
            Assert.NotNull(profile.Settings.TagBasedIcons);
            var mapping = Assert.Single(options.LibraryProfileMappings);
            Assert.Equal("existing", mapping.LibraryId);
            Assert.Equal(profile.Id, mapping.ProfileId);
            Assert.Equal(1, result.ImportedLibraryMappingCount);
            Assert.Single(result.SkippedLibraryMappings);
        }

        [Fact]
        public async Task InvalidExportSelectionDoesNotExportEveryProfile()
        {
            using var harness = await ImageQueryTests.Harness.Create();
            var response = Assert.IsType<ExportProfilesResponse>(new ExportProfilesService().Get(
                new ExportProfilesRequest { ProfileIds = "invalid-profile-id" }));
            Assert.False(response.Success);
            Assert.Null(response.JsonData);
        }

        [Theory]
        [InlineData(typeof(ImportProfilesRequest))]
        [InlineData(typeof(ExportProfilesRequest))]
        [InlineData(typeof(ValidateProfileImportRequest))]
        [InlineData(typeof(ClearTemplateCacheRequest))]
        [InlineData(typeof(RefreshCacheRequest))]
        public void AdministrativeOperationsRequireAdministratorAuthentication(Type request)
        {
            var authentication = request.GetCustomAttribute<AuthenticatedAttribute>();
            Assert.NotNull(authentication);
            Assert.Equal("Admin", authentication.Roles);
        }
    }
}

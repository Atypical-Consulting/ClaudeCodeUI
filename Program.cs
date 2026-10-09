using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using ClaudeCodeUI;
using ClaudeCodeUI.Components;
using Microsoft.AspNetCore.Localization;

if (args is ["--self-check"]) { Environment.ExitCode = SelfCheck.Run(); return; }
if (args is ["--probe-cli"]) { Environment.ExitCode = await CliProbe.Run(); return; }
if (args is ["--boot-probe", var probeDir, ..])
{
    Environment.ExitCode = await BootProbe.Run(Path.GetFullPath(probeDir), args is [_, _, var sec] && int.TryParse(sec, out var n) ? n : 90);
    return;
}

// Desktop shell (desktop/): `--desktop-port 0 --parent-pid <pid>`. Loopback HTTP only, Production,
// content root next to the executable, and stop (disposing SessionManager, so claude children die) when the shell exits.
// Once listening it prints `http://127.0.0.1:<port>/?token=<secret>` on stdout (a pipe only the shell reads): the token
// buys an HttpOnly cookie that every request (pages, assets, /_blazor) must carry, so other local users or a
// DNS-rebinding page can't drive claude.
var port = ArgValue("--desktop-port");
var desktop = port is not null;

var builder = WebApplication.CreateBuilder(new WebApplicationOptions
{
    Args = args,
    ContentRootPath = desktop ? AppContext.BaseDirectory : null,
    EnvironmentName = desktop ? Environments.Production : null,
});
var token = desktop ? RandomNumberGenerator.GetHexString(64, lowercase: true) : null;
if (desktop)
{
    builder.WebHost.UseUrls($"http://127.0.0.1:{int.Parse(port!)}");
    builder.Configuration["AllowedHosts"] = "127.0.0.1"; // HostFiltering answers 400 to rebound Host headers
}

builder.Services.AddLocalization(o => o.ResourcesPath = "Resources");
builder.Services.AddRazorComponents()
    .AddInteractiveServerComponents();
builder.Services.AddSingleton<SessionManager>();   // IAsyncDisposable: kills the claude processes on shutdown
builder.Services.AddSingleton<WorktreeService>();
builder.Services.AddScoped<UiState>();

var app = builder.Build();

if (desktop)
    app.Use(async (ctx, next) =>
    {
        if (IsToken(ctx.Request.Query["token"]))
            ctx.Response.Cookies.Append("ccui", token!, new CookieOptions { HttpOnly = true, SameSite = SameSiteMode.Strict });
        else if (!IsToken(ctx.Request.Cookies["ccui"]))
        {
            ctx.Response.StatusCode = StatusCodes.Status403Forbidden;
            return;
        }
        await next();
    });

// UI language: the .AspNetCore.Culture cookie set by /culture, else Accept-Language (the desktop webview sends the OS
// language), else English.
app.UseRequestLocalization(new RequestLocalizationOptions()
    .SetDefaultCulture("en").AddSupportedCultures(Strings.Cultures).AddSupportedUICultures(Strings.Cultures));
// ponytail: one app-wide culture for code that runs outside a request (claude's event readers trigger renders);
// two tabs in two languages would share the last one. Per-circuit capture if that ever matters.
app.Use((ctx, next) =>
{
    CultureInfo.DefaultThreadCurrentCulture = CultureInfo.CurrentCulture;
    CultureInfo.DefaultThreadCurrentUICulture = CultureInfo.CurrentUICulture;
    return next(ctx);
});

if (!app.Environment.IsDevelopment())
{
    app.UseExceptionHandler("/Error", createScopeForErrors: true);
    // The default HSTS value is 30 days. You may want to change this for production scenarios, see https://aka.ms/aspnetcore-hsts.
    if (!desktop) app.UseHsts();
}
app.UseStatusCodePagesWithReExecute("/not-found", createScopeForStatusCodePages: true);
if (!desktop) app.UseHttpsRedirection();

app.UseAntiforgery();

app.MapStaticAssets();
// Language selector (Appearance page): a plain GET form, then a full reload in the new culture.
app.MapGet("/culture", (string? c, HttpContext ctx) =>
{
    if (c is not null && Strings.Cultures.Contains(c))
        ctx.Response.Cookies.Append(CookieRequestCultureProvider.DefaultCookieName, CookieRequestCultureProvider.MakeCookieValue(new RequestCulture(c)),
            new CookieOptions { Expires = DateTimeOffset.UtcNow.AddYears(1), SameSite = SameSiteMode.Lax, IsEssential = true });
    return Results.LocalRedirect("~/settings/appearance");
});
app.MapRazorComponents<App>()
    .AddInteractiveServerRenderMode();

// No stdin watcher: a pending synchronous read on an inherited stdin pipe blocks CreateProcess (git, claude) on Windows.
if (ArgValue("--parent-pid") is { } pid)
{
    System.Diagnostics.Process parent;
    try { parent = System.Diagnostics.Process.GetProcessById(int.Parse(pid)); }
    catch (ArgumentException) { return; }   // the shell already exited during boot: nothing to serve
    _ = parent.WaitForExitAsync().ContinueWith(_ => app.Lifetime.StopApplication());
}

if (!desktop) { app.Run(); return; }
app.Start();
Console.WriteLine($"{app.Urls.First()}/?token={token}"); // Kestrel reports the port it actually bound
app.WaitForShutdown();

bool IsToken(string? value) =>
    value is not null && CryptographicOperations.FixedTimeEquals(Encoding.UTF8.GetBytes(value), Encoding.UTF8.GetBytes(token!));
string? ArgValue(string name) => Array.IndexOf(args, name) is var i and >= 0 && i + 1 < args.Length ? args[i + 1] : null;

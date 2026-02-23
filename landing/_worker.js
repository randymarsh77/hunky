export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    // Redirect /cache/nar/* requests to GitHub Releases where NARs are stored
    if (url.pathname.startsWith('/cache/nar/')) {
      const filename = url.pathname.slice('/cache/nar/'.length);
      return Response.redirect(
        `https://github.com/randymarsh77/hunky/releases/download/nix-cache/${filename}`,
        302,
      );
    }

    return env.ASSETS.fetch(request);
  },
};

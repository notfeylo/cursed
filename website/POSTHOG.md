# Download event measurement

The site contains a minimal PostHog capture client. It records only a deliberate
installer link selection and does not load the PostHog JavaScript SDK. There are
no pageview events, autocapture, cookies, local storage, session recordings,
heatmaps, surveys, user profiles, or persistent visitor IDs.

The event is named `installer download clicked` and contains:

- release version;
- selected architecture;
- the site's pathname where the selection happened;
- a new, non-persistent random event identifier.

The tracker respects the browser's Do Not Track setting. Requests go through the
same-origin `/c7-us/events` or `/c7-eu/events` Vercel rewrite. The public PostHog
project token is injected during the static build and is never a personal API
key.

## Activate it

1. Create a PostHog project and copy its public project token beginning with
   `phc_`. Never use a personal key beginning with `phx_`.
2. In PostHog, configure the project to discard IP addresses before ingestion.
3. Add `POSTHOG_PROJECT_TOKEN` to the Vercel Production environment.
4. Add `POSTHOG_REGION` as `us` or `eu`, matching the project region.
5. Update the public privacy policy to disclose PostHog and the exact event
   fields before enabling the token.
6. Redeploy, select one installer link, and confirm one
   `installer download clicked` event in PostHog Live Events.

With no `POSTHOG_PROJECT_TOKEN`, `analytics-config.js` contains an empty endpoint
and the tracker makes no request.

## Security boundary

The only CSP change required by this measurement is `connect-src 'self'` instead
of `connect-src 'none'`. Every other security directive remains unchanged. No
browser connection to a PostHog domain is permitted by CSP; Vercel proxies the
single ingestion endpoint.

# Bundled font licences

All three families are vendored as latin-subset WOFF2 files and are licensed under
the **SIL Open Font License 1.1** (<https://openfontlicense.org>). Their full licence
text is reproduced in `docs/LICENSES.md` and rendered in-app under Settings → About.

| Family         | Weights | Copyright                                                     |
| -------------- | ------- | ------------------------------------------------------------- |
| Chakra Petch   | 600 700 | © Cadson Demak                                                 |
| Inter          | 400 500 | © The Inter Project Authors                                    |
| JetBrains Mono | 400     | © JetBrains s.r.o. and the JetBrains Mono Project Authors      |

They are self-hosted rather than loaded from a CDN so the app works entirely
offline and so the Content Security Policy can keep `font-src 'self'`.

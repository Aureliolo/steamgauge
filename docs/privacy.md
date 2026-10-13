# Data and privacy

This repository holds no review data. Reviews belong to the people who wrote them and to
Valve, and they are downloadable by anyone with the app ID, so there is nothing to gain from
redistributing them here.

Nothing you download leaves your machine. The reading runs on a model on your own computer, so
a complete census needs no account and no key. Reports are the same: one file with no fonts,
scripts or stylesheets fetched from anywhere, so reading a result is not a way of publishing it.

The app makes requests to four places, and to nothing else:

- **Steam**, for the reviews of the games you add, their names and their totals, the store's
  search when you find a game by its name, and each game's store picture, fetched once and kept
  on your disk; and, when the app opens and while it stays open, how many reviews each game in the
  library has now: at most every six hours, one request per game, and Settings turns it off. The
  updates the app makes by itself fetch only what was written or edited since.
- **Hugging Face**, for the readers and the search models. Each file is fetched once, from a
  pinned commit, and checked against its pinned length and SHA-256 before it is used; the
  pipeline's `embed` command fetches its encoder the same way. When the library is checked
  against Steam, the app also asks Hugging Face whether newer releases of those models are
  published, and fetches nothing from them.
- **GitHub**, at most once a day, for whether a newer SteamGauge is out: one request to
  SteamGauge's release page, which names the newest version and nothing else. Settings turns
  it off. When you choose **Update now**, the app downloads that release's file for this
  computer and its build provenance from the release, and asks GitHub's attestations API for
  the provenance only where the release does not carry it.
- **Sigstore**, only during an update, for the current keys and certificates its signatures are
  checked against, from its public TUF repository.

A link in the window, to a review on Steam or to a release page, opens in your browser.

Steam review text, author IDs and profiles are public. A local corpus of millions of accounts
is still personal data, so it stays on your disk, and exporting a corpus offers
de-identification.

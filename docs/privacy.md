# Privacy

Reviews are downloaded to your computer and read there. Nothing you download is sent anywhere,
and reading needs no account and no key. A report is one file that fetches nothing when opened.

## Where the app connects

| To | For | How often |
|---|---|---|
| Steam | The reviews, names and totals of the games you add, store search, and each game's store picture | When you add or update a game; review totals while the app is open, at most as often as Settings chooses (every six hours unless changed, never more than hourly), which Settings turns off |
| Hugging Face | The readers and search models, each checked against its pinned length and SHA-256; whether newer ones are published | Each file once; the newer-model check whenever the library is checked against Steam |
| GitHub | Whether a newer SteamGauge is out, what each release changes, and the update itself | On opening and at most every six hours while the app is open, which Settings turns off; once more when you choose **Check for updates** on the About page, or **Update now**, which then downloads it. What each release changes, when About lists it |
| Sigstore | The keys an update's signature is checked against | Only during an update |

Nothing else. A link in the window, to a review, a release or SteamGauge's own pages, opens in
your browser.

## Your data

Review text, author IDs and profiles on Steam are public, but a library of thousands of reviewers
is still personal data, so it stays on your disk. This repository holds no review data, and the
[published dataset](https://huggingface.co/datasets/Aureliolo/game-review-claims) holds a
fingerprint of each labelled review instead of its text.

# Privacy

Reviews are downloaded to your computer and read there. Nothing you download is sent anywhere.
The app needs no account and no key. A report is one file that fetches nothing when opened.

## Where the app connects

| To | For | How often |
|---|---|---|
| Steam | The reviews, names and totals of your games, store search, each game's store picture and its developer's update posts | When you add or update a game. Review totals while the app is open, every six hours unless Settings changes or turns it off |
| Hugging Face | The readers, the search models and the writer, each checked against its pinned length and SHA-256, and whether newer ones are out | Each file once, when the work that needs it first runs. The check for newer models whenever the library is checked against Steam |
| GitHub | Whether a newer SteamGauge is out, what each release changes, and the update itself | On opening and every six hours while the app is open, which Settings turns off. Also on **Check for updates** and **Update now** |
| Sigstore | The keys an update's signature is checked against | Only during an update |

Nothing else. A link in the window opens in your browser.

## Your data

Review text and Steam profiles are public, but a library of thousands of reviewers is still
personal data, so it stays on your disk. This repository holds no review data. The
[published dataset](https://huggingface.co/datasets/Aureliolo/game-review-claims) holds a
fingerprint of each labelled review instead of its text.

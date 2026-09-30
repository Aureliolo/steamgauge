"""Small capitals reach the model as the letters they are, in training as in the reader."""

from __future__ import annotations

import train


def test_small_capitals_are_read_as_the_letters_they_are():
    assert train.plain("ᴏᴄᴄᴀꜱɪᴏɴᴀʟ ʙᴜɢꜱ ᴀɴᴅ ɢʟɪᴛᴄʜᴇꜱ") == "occasional bugs and glitches"
    assert train.plain("ʀᴇɢᴜʟᴀʀ ᴜᴘᴅᴀᴛᴇꜱ") == "regular updates"
    assert train.plain("nothing to fold, ünïcödé") == "nothing to fold, ünïcödé"


def test_every_letter_but_x_has_its_small_capital_folded():
    # x has no small capital in Unicode; every other letter must be in the table the Rust
    # reader reads too, or one side reads a letter the other leaves as a symbol.
    folded = {chr(small): plain for small, plain in train.SMALL_CAPITALS.items()}
    assert sorted(folded.values()) == sorted(set("abcdefghijklmnopqrstuvwyz"))

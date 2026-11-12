# Is the rich-text editor needed? Analysis of the diagnosis formatting

Every visit in `db.sqlite3` that is not deleted (8,447 visits) was checked
element by element: what is bold, italic or underlined, which colours, fonts,
sizes, indents and alignments appear, and where. The database was only read.

## Conclusion

**Plain text is enough.** Almost none of the formatting carries meaning the
doctor added. What matters is the structure (the template's sections, one line
per finding, numbered items) and the line breaks, and plain text keeps all of
that.

## What the formatting is used for

| Formatting | Visits using it | What it actually is |
|---|---|---|
| Section headings (Subjektif / Objektif / Assesment / Planning) | 99.5% | The template. Nine visits add a note inside a heading, e.g. "Subjektif ( alergi cipro )". |
| Bold | 93.5% | Almost always just the template's 22 labels (KU, RPS, RPD, TD, HR, BB, TB, … EKG). |
| Colour, font, size, background, indent | ~10% | Leftovers from copy-paste, with no meaning. |
| Italic | 4.6% (387) | Some Planning prescription items; meaning unclear. |
| Underline | 0.5% (44) | Warnings: allergies, positive findings. |
| Links, centring, subscript | 6 visits in total | Negligible. |

### Bold

In all but 11 visits, the only bold text is the template's 22 labels. The
doctor does not bold anything; the template does. The 11 exceptions (0.13%)
emphasise positive findings ("Wheezing: +/+", "HT (++), DM (++)",
"USG : CISTITIS") and once a note, "lupa bayar".

### Colours and fonts

The colour is `rgb(33,37,41)`, the old application's own page text colour,
together with white backgrounds and Times New Roman or Segoe UI fonts. Old
entries were copied out of the application and pasted in, carrying styles
nobody chose. Only one run anywhere uses a different colour.

### Italic: the one open question

In 386 of the 387 visits with italic, one or two Planning items are italic
while the rest are plain, so it looks deliberate. Examples: "trolac 2dd1",
"Cendo Xitrol 3dd1 gutt", "L-Bio 1dd1". Most are new that visit (200 items),
not carried over from the previous visit (80). If it means something (such as
a prescription bought elsewhere), plain text can carry it with a marker.

### Underline

The 44 underlined visits mark warnings: allergies ("Alergi sulfa",
"Alergi obat (+ ???)"), positive findings ("CHF + Edem paru +"), "lupa bayar".
Plain text can do the same with "!" or capitals.

## Other findings

- **Planning already uses plain-text markers.** 62% of visits end a Planning
  item with codes like "cxl", "cl", "cxx", "cx" or "c". They read like Roman
  numerals (C=100 … CL=150), perhaps a fee code; that is a guess. Either way,
  information is already written as text, not style.
- **Copying from the last visit is common.** 84% of follow-up visits have
  exactly the same Planning as the previous visit, and the pasted styles show
  entries being copied by hand. A "start from the previous visit" button would
  save more time than any formatting does.

## What a plain-text editor would mean

- **Kept:** all text, every line, the sections and the numbering ("1.", "2."
  stay as typed).
- **The labels can still look bold.** The application can display "KU:", "TD:"
  and the section names in bold automatically, so entries look as they do now
  without anyone typing markup.
- **Lost or changed:** italic in 387 visits, underline in 44 and hand-chosen
  bold in 11, about 5% of visits in all. Whether that matters depends on what
  the italic means.
- **Old entries:** can still be shown as they are today, or converted to plain
  text, which keeps all the words and drops only those styles.

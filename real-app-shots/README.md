# Your screenshots of the running app

Drop anything here: any name, any number, no sorting needed. This is the only folder whose
pictures come from a real session, so it is the only evidence that the wiring works, not just the
layout — see `shots/RULES.md`, "Your screenshots".

What I do with them, in this order:

1. read every file, compare each with `shots/original/<id>.png` and `shots/wip/<id>.png`;
2. rename to `<screen-id>[--note].png` and move to `shots/real/`, byte for byte — I check the sha256
   after moving, and I never retouch, crop or rescale a picture from you;
3. for each one, say in my reply what it proved, or what it proved broken;
4. a bug becomes a fix plus a case in a harness, so the same picture cannot come back.

What is worth photographing, in rough order of how blind I am about it:

- the camera: the grant, the preview, a Shell QR in focus, and the verdict it leads to;
- a real Shell plugged in: the landing card, the update screen, the transfer, the versions it reports;
- anything red: a refusal, a checksum mismatch, a download that failed — the whole window, so the
  wording and the fit are both visible;
- a second window size, if the window can even be resized.

Nothing here is deleted; it is moved to `shots/real/` once named.

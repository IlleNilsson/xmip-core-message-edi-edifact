# xmip-core-message-edi-edifact

UN/EDIFACT shape: an interchange as segments named by their tag, the first UNH message type as the announced type; the one reader of the UNA, which the EDIFACT contract reads an interchange's head by. A technology of [xmip-core-message](https://github.com/IlleNilsson/xmip-core-message).

The `UNA` is read once, here (`advice`): the six service characters it
advises, else the default set `:+.? '`, each held to ASCII because the walk
reads bytes, and a space advised as the release character read as none, as
ISO 9735 syntax 3 says (*if not used, insert space character*). The
EDIFACT contract
([xmip-core-contract-edi-edifact](https://github.com/IlleNilsson/xmip-core-contract-edi-edifact))
reads an interchange's head by it, the one technology it depends on
(ADR-0044, amendment 2026-11-15), so the two read and refuse a `UNA` in the
same words.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.

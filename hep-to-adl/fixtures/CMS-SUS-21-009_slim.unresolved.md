# Unresolved: CMS-SUS-21-009 slim fixture

These items are in the source ADL as comments or were dropped to slim the golden. They are not silent defaults.

| what | why | suggested_question |
|---|---|---|
| photon POG cut-based loose ID | commented `POGcutbasedlooseID == 1` | Which VID working point and datatier? |
| photon isolation | charged/neutral/photon sums in dR 0.03 given as comments, not a closed cut | What is the barrel/endcap isolation formula? |
| AK4/AK8 loose jet ID | commented `loosejetID == 1` | Which jet ID bit / year? |
| muon POG medium ID | commented `POGmediummuonID == 1` | Medium ID definition for this datatier? |
| electron POG veto ID | commented `POGcutbasedVetoID == 1` | Veto ID working point? |
| isolated tracks (e/μ/had) | present in the full source, omitted here | Include track vetoes in the emitted ADL? |
| HLTMETHT | commented trigger | Which HLT paths? |
| remaining b-tag table bins | table trimmed to two rows | Restore the full efficiency table? |
| remaining SR bins | EWSRs/SPSRs truncated | Emit the full bin list from the source file? |

See `CMS-SUS-21-009_slim.draft.json` for the same list in `HepToAdlDraft` form.

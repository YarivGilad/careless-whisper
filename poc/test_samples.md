# Realtime Whisper Test Samples

Use these fixed read-aloud samples when comparing chunk size, GPU/CPU, stream
mode, model size, and Hebrew vs English quality. Read at a normal speaking pace
and try to keep the microphone distance consistent between runs.

For scoring, ignore punctuation and minor capitalization differences. Focus on
missing words, substituted words, repeated phrases, and latency.

## Hebrew Sample

שלום, זהו מבחן קצר למערכת הכתבה בזמן אמת. אני מדבר בקצב רגיל, בלי למהר, ובודק אם המילים מופיעות בצורה ברורה. היום אנחנו בוחנים הקלטה מהמיקרופון, זיהוי דיבור בעברית, וזמן תגובה בין סוף המשפט לבין הופעת הטקסט. אם המשפט הזה נקלט כמו שצריך, אפשר להמשיך לשלב הבא של הניסוי.

## English Sample

Hello, this is a short test for real time dictation. I am speaking at a normal pace, without rushing, and checking whether the words appear clearly. Today we are testing microphone capture, local Whisper transcription, and the delay between the end of a sentence and the moment the text appears. If this paragraph is captured correctly, we can move to the next stage of the experiment.

## Suggested Runs

Run English first as the control case. It should be easier to judge whether the
pipeline works before tuning Hebrew.

Continuous-capture English baseline:

```sh
python3 poc/realtime_mic_poc.py --gpu --chunk-seconds 5 --max-chunks 8 --keep-audio
```

Older sequential-capture comparison:

```sh
python3 poc/realtime_mic_poc.py --capture-mode chunked --gpu --chunk-seconds 5 --max-chunks 8 --keep-audio
```

Stream English baseline:

```sh
poc/run_whisper_stream.sh
```

Continuous-capture Hebrew comparison:

```sh
python3 poc/realtime_mic_poc.py --language he --gpu --chunk-seconds 5 --max-chunks 8 --keep-audio
```

Stream Hebrew comparison:

```sh
poc/run_whisper_stream.sh -l he
```

Capture the command output after each run and compare it to the expected sample
text above.

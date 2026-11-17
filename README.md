# Keycard Shell Updater

Keycard Shell Updater is an application to update Shell firmware and database, and verify its authenticity. Here are some screenshots of its functionality.

<img width="1976" height="1602" alt="Screenshot From 2026-11-17 14-09-19" src="https://github.com/user-attachments/assets/de08205f-38a0-4171-bdd0-1c25eb069182" />

<img width="1976" height="1602" alt="Screenshot From 2026-11-17 14-09-39" src="https://github.com/user-attachments/assets/e7bd0340-b373-4c90-9236-152da66c7490" />

<img width="1976" height="1602" alt="Screenshot From 2026-11-17 14-09-48" src="https://github.com/user-attachments/assets/9513c263-150a-4982-9524-609d65d3244b" />

<img width="1976" height="1602" alt="Screenshot From 2026-11-17 14-10-10" src="https://github.com/user-attachments/assets/a82a8012-9740-473f-9759-8a247a93e48e" />

## macOS

The build is unsigned, so the first run reports it as damaged. Clear that and it opens:

    xattr -cr "/Applications/Keycard Shell Updater.app"

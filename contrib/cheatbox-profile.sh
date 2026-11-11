# /etc/profile.d/cheatbox.sh — случайная шпаргалка при интерактивном входе по SSH.
# Работает в сессии пользователя: знает ширину терминала и умеет в цвета.
case $- in *i*) ;; *) return 0 2>/dev/null ;; esac
if [ -n "$SSH_CONNECTION" ] && [ -t 1 ] && command -v cheatbox >/dev/null 2>&1; then
    cheatbox random --boxes 2
    # Вариант покороче — одна строка:
    # cheatbox tip
    # Только нужные листы:
    # cheatbox random --sheets linux,docker,tmux --boxes 2
fi

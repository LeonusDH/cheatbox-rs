# cheatbox (Rust)

Шпаргалки «в бенто-коробке» прямо в терминале — порт [fatinul/cheatbox](https://github.com/fatinul/cheatbox) с Python на Rust,
плюс режим для MOTD / приветствия при входе по SSH.

- один бинарник без Python, `uv`, `rich` и `pick`; листы встроены внутрь;
- 14 встроенных листов: из оригинала — `linux`, `docker`, `kubernetes`, `npm`, `bun`, `tmux`;
  новые — `podman`, `git`, `systemd`, `cargo`, `gradle` (с блоком про моддинг Minecraft), `ssh`, `ufw`, `nftables`;
- формат `*.json` **полностью совместим** с оригиналом (`LOGO`, `TITLE`, `STYLE`, категории) — листы из upstream кладутся как есть;
- разметка `rich` в логотипах: `[bold red]…[/]`, `[on blue]`, `[#ff8800]`, все 256 именованных цветов (`misty_rose3`, `navajo_white1`…);
- подстраивается под ширину терминала (две колонки → одна), колонки всегда ровные.

## Отличия от оригинала

| | оригинал | этот порт |
|---|---|---|
| `kubectl get [resource]` | `[resource]` пропадает (rich считает его тегом стиля) | печатается как есть |
| описание переносится на 2 строки | колонки съезжают | высоты считаются по факту, низы ровные |
| раскладка | по очереди лево/право | в более короткую колонку (masonry) |
| ширина колонки команд | общая на весь лист | своя у каждого блока — меньше переносов |
| узкий терминал | ломается | одна колонка, лого убирается |
| неинтерактивный запуск | только меню `pick` | `show`, `random`, `tip` |

## Использование

```
cheatbox                      # меню выбора (с fuzzy-поиском)
cheatbox list                 # какие листы есть
cheatbox show docker          # конкретный лист
cheatbox show linux -o net -o perm     # только категории, содержащие «net»/«perm»
cheatbox random --boxes 2     # случайный лист, 2 случайных блока (алиас: motd)
cheatbox random -s linux,tmux # случайный из выбранных
cheatbox tip                  # одна случайная команда одной строкой
cheatbox path                 # куда класть свои листы
```

Общие флаги: `--width N`, `--color auto|always|never` (уважается `NO_COLOR`), `--no-header`, `--data-dir DIR`.

### Свои листы

Положите `имя.json` в каталог из `cheatbox path`
(Linux: `~/.config/cheatbox/`, Windows: `%APPDATA%\cheatbox\`) или в `$CHEATBOX_DIR`.
Файл с тем же именем, что и встроенный, его переопределяет.

Чтобы встроить лист в бинарник, положите его в `sheets/` и пересоберите (`build.rs` следит за папкой).
Новые листы совместимы и с оригинальным Python-cheatbox.

## MOTD при входе по SSH

**Вариант 1 — `/etc/profile.d` (рекомендую).** Выполняется уже в сессии пользователя,
поэтому знает ширину терминала:

```sh
sudo install -m755 target/release/cheatbox /usr/local/bin/
sudo cp contrib/cheatbox-profile.sh /etc/profile.d/cheatbox.sh
```

**Вариант 2 — `update-motd.d`** (Debian/Ubuntu). Выводится вместе с остальным MOTD,
но запускается до сессии, так что ширина задаётся жёстко:

```sh
sudo install -m755 contrib/60-cheatbox /etc/update-motd.d/60-cheatbox
```

## Сборка

```
cargo build --release
```

Готовые бинарники для Linux (musl, x86_64/aarch64), Windows и macOS собирает workflow
`.github/workflows/release.yml` — запускается вручную через Actions.

## Лицензия

GPL-3.0-or-later, как и оригинал. Листы linux/docker/kubernetes/npm/bun/tmux взяты из fatinul/cheatbox, остальные написаны для этого порта.

# chess2lichess GitHub 업로드 가이드

## 1) 빌드 확인

```bash
cargo build --release
```

## 2) 기본 실행

```bash
./target/release/c2l "https://www.chess.com/game/live/123456789"
```

## 3) GitHub 업로드

```bash
git add .
git commit -m "feat: initial import"
git push -u origin main
```

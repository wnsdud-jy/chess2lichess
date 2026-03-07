# @wnsdud-jy/c2l Release Guide

`@wnsdud-jy/c2l`를 GitHub Release + npm에 배포하는 실전 절차입니다.

## 0. 사전 준비

- GitHub 저장소: `wnsdud-jy/chess2lichess`
- npm 패키지명: `@wnsdud-jy/c2l`
- 로컬에서 로그인 확인:

```bash
npm whoami
```

## 1. 변경사항 커밋/푸시

아직 커밋하지 않은 패키징 변경을 커밋하고 원격에 푸시합니다.

```bash
git status
git add .
git commit -m "feat: package c2l as @wnsdud-jy/c2l with release workflows"
git push origin main
```

## 2. GitHub Secrets 설정

GitHub 저장소에서 아래 시크릿을 추가합니다.

- 경로: `Settings > Secrets and variables > Actions > New repository secret`
- 이름: `NPM_TOKEN`
- 값: npm access token

토큰 생성 위치:
- `https://www.npmjs.com/settings/<your-account>/tokens`
- 권장: `Automation` 타입 토큰

## 3. 버전 동기화 확인

`Cargo.toml`과 `package.json` 버전이 같아야 워크플로우가 통과합니다.

```bash
cargo_version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
npm_version=$(node -p "JSON.parse(require('node:fs').readFileSync('package.json','utf8')).version")
echo "cargo=$cargo_version npm=$npm_version"
```

두 값이 다르면 먼저 맞춘 뒤 커밋/푸시하세요.

## 4. 태그 푸시로 바이너리 릴리즈 생성

예시 버전이 `0.1.1`이라면:

```bash
git tag v0.1.1
git push origin v0.1.1
```

이후 `release-binaries` 워크플로우가 자동 실행되어 아래 6개 파일을 릴리즈에 업로드합니다.

- `c2l-v0.1.1-linux-x64`
- `c2l-v0.1.1-linux-arm64`
- `c2l-v0.1.1-darwin-x64`
- `c2l-v0.1.1-darwin-arm64`
- `c2l-v0.1.1-win32-x64.exe`
- `c2l-v0.1.1-checksums.txt`

## 5. npm publish 실행

기본적으로 GitHub Release가 `published` 되면 `npm-publish` 워크플로우가 자동 실행됩니다.

자동 실행이 안 됐으면 Actions 탭에서 `npm-publish`를 `Run workflow`로 수동 실행하고 `tag`에 `v0.1.1` 입력합니다.

## 6. 배포 검증

### 6-1. npm 레지스트리 확인

```bash
npm view @wnsdud-jy/c2l version
```

### 6-2. 실제 설치/실행 확인

```bash
npm i -g @wnsdud-jy/c2l
c2l --help
```

또는:

```bash
npx @wnsdud-jy/c2l --help
```

## 7. 문제 발생 시 체크포인트

- `release-binaries` 실패:
  - 태그 형식이 `vX.Y.Z`인지 확인
  - `Cargo.toml`/`package.json` 버전 일치 확인
- 설치 시 다운로드 실패:
  - 릴리즈에 대상 플랫폼 asset 존재 여부 확인
  - `c2l-vX.Y.Z-checksums.txt` 파일 존재 여부 확인
- `npm-publish` 실패:
  - `NPM_TOKEN` 유효성/권한 확인
  - 스코프 `@wnsdud` 배포 권한 확인

## 8. 다음 릴리즈부터 반복 절차

1. 버전 올리기 (`Cargo.toml`, `package.json`)
2. 커밋/푸시
3. 새 태그 푸시 (`vX.Y.Z`)
4. Actions 성공 확인
5. npm 설치 스모크 테스트

# 에이전트 실행·병렬화·통합 계약

목표는 agent 수를 늘리는 것이 아니라 사람이 의미를 놓치지 않으면서 검증된 변경을 빨리 통합하는 것이다. [AGENTS.md](../../AGENTS.md)의 기존 원칙을 대체하지 않고 구체화한다. 이 문서는 현재 도구나 계정에 없는 권한을 부여하지 않는다.

## 1. 작업 시작

최신 remote main, local 변경, AGENTS, STATUS, ROADMAP, tasks.json을 확인한다. 모든 과거 대화·ZIP을 문맥에 넣지 않는다. 현재 task의 계약·관련 타입·fixture·최근 변경만 읽는다. 구현된 API를 다시 만드는 대신 연결되지 않은 사용자 흐름을 찾는다.

CN-001이 toolchain, package lock, 네트워크/인증의 실제 사용 가능 여부와 기준 검사를 확인한다. read 권한과 write 도구 존재, GitHub object 생성과 branch 반영, compile과 source inspection을 구분한다. credential을 채팅/소스/log에 넣지 않는다.

## 2. 역할과 병렬 작업

- Coordinator/integrator: task 배정, 계약 freeze, 승인 blocker, worktree 충돌, 전체 CI와 main 갱신을 담당한다. 동시에 여러 통합자가 main을 갱신하지 않는다.
- Builder: 자기 task와 write scope만 구현한다. 다른 agent의 미완성 branch나 승인되지 않은 interface에 임의 의존하지 않는다.
- Verifier: 독립 사례/오류/회귀/복구를 검사한다. 새 구현 출력으로 oracle을 재생성하지 않는다.
- Release/operator: artifact, 설치/지원 매트릭스, 복구 훈련과 릴리스 준비를 담당한다. production 권한은 별도 부여다.

한 agent가 역할을 번갈아 맡을 수 있지만 같은 컨텍스트의 자기 검사를 독립 검토라고 부르지 않는다. 먼저 builder 2개 + verifier 1개를 병렬로 사용하고, 계약/타입/문법 변경 lane은 한 개로 제한한다. 대기/충돌/재작업/검토 시간을 관측한 뒤 확장한다.

각 작업은 독립 worktree/branch를 사용한다. shared schema, public IR, serialization, root manifest, workflow, language semantics의 동시 쓰기를 금지한다. 모듈 이름이 달라도 public 타입을 동시에 바꾸면 충돌이다. 변경 필요 시 coordinator가 interface-first task를 통합하고 소비자들을 다시 기준화한다.

## 3. 작업표 상태와 선택

상태는 `planned`, `in_progress`, `blocked`, `in_review`, `done`, `deferred`다. `planned`는 즉시 착수 가능하다는 뜻이 아니다. 모든 dependency의 증거와 해당 decision blocker를 확인하고, 충돌하지 않는 write scope를 배정해야 한다. 다른 agent가 진행 중인 task를 claim하지 않는다.

큰 task는 [task template](templates/task.md)으로 하위 단위로 나눈다. 단위는 구현·독립 검사·검토를 한 묶음으로 끝낼 수 있는 크기다. 선언/구현/test를 에이전트별로 무작정 나누는 대신 고정된 interface를 기준으로 분리한다. 초안 경계는 한 의미 변경, 보통 3~10개 수작업 파일이며 기계적 변환은 별도로 설명한다. 줄 수는 출시 기준이 아니다.

claim에는 owner, base SHA, write scope, 현재 hypothesis와 acceptance를 기록한다. `done`에는 정확한 commit·명령·환경·결과·원격 검증·제한과 인수인계가 필요하다. 선행 task의 일부만 완료되었다면 dependency를 거짓으로 done 처리하지 말고 세분화한다.

## 4. 위험과 결정

| 수준 | 예시 | 통합 조건 |
|---|---|---|
| R0 | 링크/문서 오류, 의미 불변의 작은 정리 | 관련 검사와 diff 확인 |
| R1 | 승인된 계약 내부 구현, 독립 테스트, 비파괴 연결 | 관련·전체 검사, 독립 review, 현재 Git 허용 범위 |
| R2 | 기존 언어/ID/직렬화/정답/공개 API 의미 변경 | ADR·이전 기준·이행·owner disposition; 무관한 작업은 계속 |
| R3 | 실제 데이터 삭제/이행, 외부 송신, 권한·비밀·비용·출시 | 명시적 해당 작업 승인과 복구/검증 계획 |

기존 허용 범위 내 세부 구현은 매번 사람에게 질문하지 않는다. 모호한 사항은 권장안·대안·영향·되돌림을 한 번에 정리한다. 승인 pending은 추측해 채우지 않고 blocker로 남긴다. 사람이 마지막에 QA하더라도 핵심 정책/새 비용/데이터 권한까지 자동 위임한 것으로 해석하지 않는다.

## 5. 현재 실행 가능한 검증 명령

Rust toolchain과 잠금 의존성이 준비된 checkout에서:

```sh
cargo test --workspace --locked --offline
cargo test --workspace --release --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo build --workspace --locked --offline
npm ci --ignore-scripts
npm test
node scripts/check-native-conformance.mjs
node scripts/check-bound-conformance.mjs
cargo run --locked --offline -p cannon-core --example project
cargo run --locked --offline -p cannon-core --example review
node dist/src/cli.js demo
```

`--offline`은 toolchain을 설치하지 않는다. 라이브러리 다운로드가 필요해 실패한 상황을 언어 테스트 실패/성공으로 바꾸지 않는다. 고정 의존성 대신 preinstalled 버전을 썼다면 그 차이를 evidence에 쓴다. local tool이 없을 때 허용된 exact-commit remote CI를 사용하고 실제 결과를 읽는다. 검사 조건/테스트 기대값을 약화해 녹색을 만들지 않는다.

현재 아직 없는 format/fuzz/packaging/E2E 검사는 담당 task가 실제 명령과 CI를 추가한다. 이 문서에 미래 명령을 성공했다고 복사하지 않는다.

## 6. main 통합 절차

소유자는 이번 계획 문서의 main 반영을 요청했다. 향후 구현에서는 현재 사용자 허용과 저장소 규칙을 다시 따른다. 보호 규칙이 PR을 요구하면 우회하지 않는다. 직접 반영이 허용되면 PR은 필수가 아니지만 아래 검증은 필수다.

1. 최신 main에서 작은 변경을 만들고 baseline와 diff를 고정한다.
2. local 검사 또는 임시 검증 branch의 exact-commit CI를 실행한다. build/test만 수행하고 자동 배포하지 않는다.
3. verifier가 의미·정답 기준·문서·한도·권한·의존성을 검토한다.
4. main을 재조회한다. 앞서간 커밋이 있으면 보존해서 재통합하고 새 정확한 commit을 다시 검사한다.
5. 검증된 descendant만 fast-forward한다. force push, 다른 변경 삭제, 보호 규칙 완화는 하지 않는다.
6. main SHA와 필요한 원격 검사를 다시 읽고, 무엇을 확인했는지 보고한다. 객체 생성만으로 push 성공이라 하지 않는다.

실패 시 이전 검증 main을 유지한다. main이 이미 문제 커밋을 받았다면 원인을 분석하고 revert/수정의 영향을 확인한 후 새 커밋으로 처리한다. history rewrite는 복구의 기본 경로가 아니다.

## 7. 효율적인 문맥과 작업 예산

각 agent의 context pack은 task ID, base SHA, 관련 계약·타입 경로, fixture, 수정 가능한 경로, non-goals, 예상 실패를 포함한다. 컨텍스트가 커지면 코드/근거의 위치를 남기고 과거 채팅을 압축해 정답으로 쓰지 않는다.

cache는 toolchain/dependency/정확한 입력에 맞게 재사용한다. semantic 결과 캐시의 키가 불명확하면 재검사한다. 작은 관련 검사를 먼저 돌린 다음 통합 전 전체 matrix를 실행한다. 관련 검사 통과만으로 production 완료로 표시하지 않는다.

token/실행 비용/도구 호출 한도는 task budget에 기록한다. 동일 원인으로 2회 연속 검증 실패하면 변경 범위를 더 키우지 말고 최소 재현과 계약을 재검토한다. 이는 숨긴 시간 약속이나 자동 재시도 승인과 다르다.

## 8. 중단과 인수인계

도구의 보안 판정 차단, 인증 부재, unsupported operation은 서로 다르게 기록한다. 차단된 내용을 다른 경로/인코딩/권한으로 보내지 않는다. 접근 가능한 독립 작업은 계속하되 완료 범위와 미반영 범위를 섞지 않는다.

[h​andoff template](templates/handoff.md)에 구현/검사/실제 반영/미확인/다음 최소 작업을 남긴다. 실행하지 않은 Rust 테스트를 작성 개수로 성공 보고하지 않는다. demo는 의도적으로 실패하는 후보를 잘 검출해서 exit 0일 수 있으므로 해석도 명시한다.

### 종료 보고의 한 문장

`Task CN-xxx: <변경>. Evidence: <정확한 commit/환경/검사>. Delivery: <원격 main SHA 또는 미반영>. Unknowns: <남은 경계>.`

# Cannon

**사람이 모델과 정책을 직접 다루고, AI의 제안을 실행 근거와 함께 검토하는 프로그래밍 시스템의 프로토타입.**

정책을 AI가 다른 구현으로 다시 번역하지 않고 직접 실행합니다. 이름·버전·소스 위치를 구분하고, 아키텍처 변경·기대값 변경·검증 결과를 별개로 보여줍니다.

현재 구현 경로는 두 가지입니다. **Rust**는 타입 입력, 정책 연결과 모듈 경계, 실행 추적·취소, 변경 검토와 버전별 세션을 제공합니다. **TypeScript 기준 구현**은 단일 `.intent` 파일의 레코드·불변조건·규칙·상태 전이와 기존 CLI를 제공합니다. npm의 `cannon` 명령을 Rust로 바꾼 것은 아닙니다. 범용 언어, 완성된 IDE, 운영 저장소 또는 실제 AI 제공자 연결은 아직 아닙니다. 정확한 범위는 [STATUS.md](STATUS.md)에 있습니다.

## Rust로 실행하기 — Node 불필요

저장소는 Rust 1.85.1을 고정합니다. 아래 `--offline` 명령은 Rust 도구 자체를 설치하지 않으므로 해당 도구가 먼저 설치되어 있어야 합니다. 네이티브 코어는 표준 라이브러리만 사용합니다.

```bash
git clone https://github.com/eunhhu/cannon.git
cd cannon
cargo test --workspace --locked --offline
cargo run --locked --offline -p cannon-core --example project
```

새 프로젝트 예제는 **재고 계산 → 예약 결정 → 배송비 계산**을 연결합니다. Checkout은 Inventory가 공개한 정책 결과만 사용하며 원본 재고 입력이나 비공개 정책을 직접 연결하면 컴파일 단계에서 거절합니다. 세 정책의 실제 중간값과 소스 위치를 출력하고, 후보가 정책과 기대값을 함께 바꿔도 원래 사례의 실패를 드러냅니다.

```text
Candidate's own case: Passed
Preserved original case: Mismatch
Historical failed outputs: ["canReserve", "shipping"]
Rejected architecture change before execution: PRIVATE_POLICY ...
```

정책 표현식은 [examples/projects/inventory/](examples/projects/inventory/), 모듈 연결은 [Rust 예제](crates/cannon-core/examples/project.rs)에 있습니다. 현재는 표현식 파일을 빌드할 때 포함하는 호스트 API 예제이며, 런타임 프로젝트 파일 로더나 전체 `.intent` Rust 파서는 아닙니다. [프로젝트 계약](docs/native-projects.md)에 eager dataflow, 소유권 검사, 타입 경계, 누적 실행 예산과 검토 범위를 기록했습니다.

기존 네이티브 예제도 유지합니다.

```bash
cargo run --locked --offline --bin cannon-native -- eval '8 + 2 <= 10' --json
cargo run --locked --offline --bin cannon-policy -- demo --json
cargo run --locked --offline -p cannon-core --example observe
cargo run --locked --offline -p cannon-core --example review
```

[타입 정책](docs/native-bound-policies.md), [관측·취소](docs/native-observation.md), [버전별 세션](docs/native-sessions.md)은 같은 Rust 실행기를 사용합니다. 새 프로젝트 계층도 별도 계산기를 만들지 않습니다.

## TypeScript 기준 구현 실행

이 경로에는 Node.js 22.16 이상이 필요합니다. 실행 시 외부 패키지는 없고 개발 의존성은 잠금 파일에 고정합니다.

```bash
npm ci --ignore-scripts
npm test
npm run demo
```

Git에는 `dist/`, `target/`, `node_modules/`를 넣지 않습니다. 소스를 바꾸면 해당 구현을 다시 빌드해야 합니다. 기존 `npm run cannon -- <command>`와 `intent` 별칭은 유지합니다. npm 레지스트리에 게시하지 않습니다(`private: true`).

기준 데모는 예약 조건의 `<=`를 `<`로 바꾸고 테스트 기대값까지 수정한 후보와 비교합니다.

```text
Candidate scenarios: 4/4 passed
Historical expectations replayed against the candidate:
  FAILED 남은 재고를 모두 예약할 수 있다 — actual outcome changed
```

후보 테스트가 모두 통과해도 이전 입력과 기대값으로 다시 확인합니다. 테스트 수정이나 삭제가 과거 요구사항의 실패를 숨기지 못하게 합니다. 데모는 오래된 제안 거부와 예시용 메모리 저장소 충돌도 보여줍니다. 실제 데이터베이스·AI 제공자·운영 배포는 호출하지 않습니다.

## 기준 구현의 실행 가능한 모델과 규칙

```intent
module Inventory

@id("stock")
record Stock {
    @id("stock.onHand") onHand: NonNegativeInt
    @id("stock.reserved") reserved: NonNegativeInt
    invariant reserved <= onHand
}

@id("canReserve")
rule canReserve(stock: Stock, quantity: PositiveInt) -> Bool =
    stock.reserved + quantity <= stock.onHand
```

안정적인 `@id`는 이름을 바꿔도 같은 개념을 식별합니다. 기본 타입과 참조는 정적으로 검사하고, 정수 범위·정제 타입·레코드 불변조건은 실행 경계에서 검사합니다. 타입 검사와 테스트 통과를 수학적 증명으로 표시하지 않습니다. 위 전체 선언 문법은 아직 TypeScript 기준 구현의 범위입니다.

전체 예시는 [examples/inventory.intent](examples/inventory.intent), 정확한 의미는 [spec/semantics.md](spec/semantics.md)에 있습니다.

## 기준 CLI와 검증 근거

```bash
npm run build
npm run cannon -- check examples/inventory.intent
npm run cannon -- inspect examples/inventory.intent
npm run cannon -- run examples/inventory.intent --trace case.exact
npm run cannon -- diff examples/inventory.intent examples/inventory.changed.intent --fail-on-regression
```

`diff --fail-on-regression`은 회귀가 있으면 종료 코드 1을 반환합니다. `run --json`은 버전이 연결된 근거를 출력합니다.

```bash
mkdir -p .intent
npm run cannon -- run examples/inventory.intent --evidence .intent/baseline.json
npm run cannon -- evidence examples/inventory.intent .intent/baseline.json
```

보고서는 소스·입력값을 포함할 수 있으며 기존 파일을 덮어쓰지 않습니다. `.intent/`는 Git에서 제외합니다. 보고서 해시는 우발적 변경과 현재성 검사에 쓰며 서명된 인증서는 아닙니다. Rust의 메모리 객체 핸들은 이 영속 보고서 형식과 별개입니다.

## 사람과 AI의 같은 제안 경로

```bash
npm run cannon -- propose examples/inventory.intent examples/inventory.changed.intent \
  --reason "재고를 한 개 남기는 정책 검토" --author assistant --out .intent/proposal.json
npm run cannon -- preview examples/inventory.intent .intent/proposal.json
```

정확한 `Review ID`를 확인한 뒤 명시적으로 적용합니다.

```bash
npm run cannon -- apply examples/inventory.intent .intent/proposal.json --review <검토한-ID>
```

이 `apply`는 실제 파일을 바꿉니다. 반면 Rust 세션의 `apply`는 메모리 상태만 바꿉니다. 어느 쪽이든 승인과 검증 성공은 별개이며, 이유와 author 필드는 인증된 사실이 아닙니다. 실제 AI 제공자 연결이나 자동 소스 전송은 없습니다. [연동 계약](spec/assistant-contract.md)을 기준으로 외부 어댑터를 연결해야 합니다.

## 개발과 검증

Rust 구현은 `crates/cannon-core/`, TypeScript 기준 의미 엔진은 `src/core/`, 기준 파서는 `src/language/`에 있습니다. Rust 프로젝트 테스트는 모듈 접근 위반, 순환, 잘못된 연결, 실제 소스 위치, 취소·누적 예산, 변경/삭제된 후보 사례와 재고 입력 3060가지 조합을 검사합니다. 기존 Rust 세션 테스트, TypeScript 90개 검사와 두 엔진의 독립 적합성 사례도 유지합니다.

GitHub Actions는 Rust debug/release 테스트·엄격한 Clippy·빌드·대조 검사·데모를 Linux/Windows/macOS에서 수행하며, 별도로 Node 22/24 기준 검사를 유지합니다. 실제 성공 여부는 정확한 커밋의 Actions 결과를 확인해야 합니다. [STATUS.md](STATUS.md)와 과거 `VALIDATION.json`을 새 실행 결과로 혼동하지 않습니다.

이전 전체 파일 Rust 후보와 편집기 전달 제한은 [spec/delivery.md](spec/delivery.md)에 남아 있습니다. 이 저장소는 차단된 내용을 다른 형식으로 재전송하지 않습니다. 전체 `.intent` Rust 파서, 레코드/상태 전이, 실제 편집기·AI 연결과 운영 저장소는 아직 완료하지 않았습니다.

[아키텍처](spec/architecture.md), [실행 의미](spec/semantics.md), [설계 결정](spec/decisions.md), [작업 원칙](AGENTS.md)을 함께 읽으세요. 별도의 오픈소스 라이선스는 아직 부여하지 않았습니다(`UNLICENSED`).

**AI와의 대화가 없어도 무엇을 결정했고, 무엇이 바뀌었고, 무엇을 검증하지 않았는지 프로젝트에서 확인할 수 있어야 합니다.**

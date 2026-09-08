# Cannon

**사람이 모델과 정책을 직접 다루고, AI의 제안을 실행 근거와 함께 검토하는 프로그래밍 시스템의 프로토타입.**

`.intent` 파일이 사람이 읽는 설계이자 실제 실행 코드입니다. AI가 규칙을 다른 언어로 다시 번역하지 않습니다. 이름·버전·소스 위치를 구분하고, 구조 변경·기대값 변경·검증 결과를 별개로 보여줍니다.

현재 저장소는 **0.1.0 / 단일 파일·단일 모듈의 CLI와 공통 의미 엔진**입니다. 범용 언어, 운영용 저장소, 완성된 IDE 또는 연결된 AI 제품은 아닙니다. 구현과 확인 범위는 [STATUS.md](STATUS.md), 이번 업로드의 제한은 [spec/delivery.md](spec/delivery.md)에 기록했습니다.

## 실행

Node.js 22.16 이상이 필요합니다. 개발 의존성은 잠금 파일에 고정되어 있고, 실행 시 외부 패키지는 필요하지 않습니다.

```bash
git clone https://github.com/eunhhu/cannon.git
cd cannon
npm ci
npm test
npm run demo
```

Git에는 `dist/`와 `node_modules/`를 넣지 않습니다. `npm test` 또는 `npm run build`로 실행 파일을 만듭니다. 소스를 수정한 뒤에는 다시 빌드해야 합니다.

프로토타입 이름은 Cannon이며 `.intent` 파일 형식은 유지합니다. `npm run cannon -- <command>`를 사용할 수 있고 기존 `intent` 별칭도 남겨두었습니다. npm 레지스트리에 게시하지 않습니다(`private: true`).

## 먼저 볼 데모

`npm run demo`는 재고 예약 사례 네 개를 실행한 다음, 예약 조건의 `<=`를 `<`로 바꾸고 테스트 기대값까지 수정한 후보와 비교합니다.

```text
Candidate scenarios: 4/4 passed
Historical expectations replayed against the candidate:
  FAILED 남은 재고를 모두 예약할 수 있다 — actual outcome changed
```

후보 테스트가 모두 통과해도 이전 입력과 기대값으로 다시 확인합니다. 테스트를 수정하거나 삭제해서 과거 요구사항의 실패가 사라지지 않습니다. 데모는 오래된 제안 거부와 메모리 저장소의 버전 충돌도 보여줍니다. 실제 데이터베이스·AI 제공자·운영 배포는 호출하지 않습니다.

## 실행되는 모델과 규칙

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

안정적인 `@id`는 이름을 바꿔도 같은 개념을 식별합니다. 기본 타입과 참조는 정적으로 검사하고, 정수 범위·정제 타입·레코드 불변조건은 실행 경계에서 검사합니다. 타입 검사와 테스트 통과를 수학적 증명으로 표시하지 않습니다.

전체 예시는 [examples/inventory.intent](examples/inventory.intent), 정확한 실행 의미는 [spec/semantics.md](spec/semantics.md)에 있습니다.

## CLI

```bash
npm run build
npm run cannon -- check examples/inventory.intent
npm run cannon -- inspect examples/inventory.intent
npm run cannon -- run examples/inventory.intent --trace case.exact
npm run cannon -- diff examples/inventory.intent examples/inventory.changed.intent
```

회귀가 있을 때 종료 코드 1을 반환하게 하려면 `diff`에 `--fail-on-regression`을 붙입니다. `run --json`은 버전이 연결된 실행 근거를 출력합니다.

```bash
mkdir -p .intent
npm run cannon -- run examples/inventory.intent --evidence .intent/baseline.json
npm run cannon -- evidence examples/inventory.intent .intent/baseline.json
```

보고서는 소스·입력값을 포함할 수 있으며 기존 파일을 덮어쓰지 않습니다. `.intent/`는 Git에서 제외합니다. 보고서 해시는 신선도와 우발적 변경 검사에 사용되며 서명된 인증서는 아닙니다.

## 사람과 AI의 같은 제안 경로

```bash
npm run cannon -- propose examples/inventory.intent examples/inventory.changed.intent \
  --reason "재고를 한 개 남기는 정책 검토" --author assistant --out .intent/proposal.json
npm run cannon -- preview examples/inventory.intent .intent/proposal.json
```

검토 화면의 정확한 `Review ID`를 지정해야 적용할 수 있습니다.

```bash
npm run cannon -- apply examples/inventory.intent .intent/proposal.json --review <검토한-ID>
```

`apply`는 실제 소스를 바꿉니다. 기준 스냅샷이 오래되었거나 후보에 정적 오류가 있으면 거절합니다. 승인은 검증 성공과 별개입니다. 제안 이유는 검증되지 않은 설명으로 표시합니다.

라이브러리에는 `compile`, `inspectProject`, `inspectDefinition`, `invoke`, `runScenario`, `verify`, `compare`, `createProposal`, `previewChange`, `applyChange`가 있습니다. 실제 AI 제공자 연결이나 자동 소스 전송은 없습니다. [연동 계약](spec/assistant-contract.md)을 기준으로 어댑터를 추가할 수 있습니다.

## 개발과 검증

`src/core/`는 의미 엔진, `src/language/`는 파서, `src/cli.ts`는 소비자입니다. `test/`의 90개 검사는 정상 동작뿐 아니라 오류, 이름 변경, 기대값 수정·삭제, 오래된 제안, 파일 적용을 검사합니다. 재고 입력 3,060가지 조합도 포함됩니다.

Cannon 전체 로컬 패키지의 95개 검사 중 편집기 관련 5개는 이 원격 전달본의 범위에 포함하지 않았습니다. 편집기 업로드가 도구에서 차단되었기 때문이며, 빠진 파일을 참조하는 깨진 패키지를 남기지 않도록 CLI 전달본을 분리했습니다. 이는 편집기 검증이 성공했다는 뜻이 아닙니다.

GitHub Actions는 Node.js 22와 24에서 잠금 파일 기반 설치·빌드·테스트·데모를 실행하도록 설정되어 있습니다. 실제 결과는 Actions 탭에서 확인하며 로컬 검사 기록과 구분합니다.

[아키텍처](spec/architecture.md), [실행 의미](spec/semantics.md), [설계 결정](spec/decisions.md), [작업 원칙](AGENTS.md) 순서로 읽으면 됩니다. 별도의 오픈소스 라이선스는 아직 부여하지 않았습니다(`UNLICENSED`).

**AI와의 대화가 없어도 무엇을 결정했고, 무엇이 바뀌었고, 무엇을 검증하지 않았는지 프로젝트에서 확인할 수 있어야 합니다.**

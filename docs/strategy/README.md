# Cannon production 전략: 에이전트 시작점

2026-09-09 작성. 이 디렉터리는 **미래 목표·작업 계약·출시 준비**를 관리한다. 구현 사실은 루트 `STATUS.md`와 정확한 커밋의 테스트/실행 결과가 원본이다. 목표를 작성했다고 기존의 `not implemented` 상태를 지우지 않는다.

**전달 범위:** 상세 아키텍처·언어·결정 기록 보충은 업로드가 차단되어 안내 페이지만 있다. [DELIVERY.md](DELIVERY.md)를 읽고 이를 완성된 설계로 간주하지 않는다. 현재 동작은 기존 spec/native 계약, 향후 실행 계획은 아래 실제 문서를 따른다.

## 필요한 문서만 읽기

| 목적 | 문서 |
|---|---|
| 목표·9단계·완료 기준 | [ROADMAP](../../ROADMAP.md) |
| 시작 시점의 실제 기능과 검증 | [baseline](baseline.md) |
| 시험 전략·성능 목표·출시 게이트 | [quality-release](quality-release.md) |
| 보안·개인정보·운영·사고·복구 | [security-operations](security-operations.md) |
| 에이전트 배정·병렬 작업·통합·보고 | [agent-execution](agent-execution.md) |
| 선행조건·수정 범위·완료 증거 | [tasks.json](tasks.json) |
| 작업 생성·인수인계 | [task template](templates/task.md), [handoff template](templates/handoff.md) |
| 외부 표준과 조회한 근거 | [sources](sources.md) |
| 상세 설계 보충의 미전달 안내 | [architecture](architecture.md), [language-runtime](language-runtime.md), [decisions](decisions.md) |

## 권위와 변경

현재 실행 의미는 `spec/semantics.md` 및 기존 native 계약을 따른다. 계획 속 새 기능은 구현·버전·검증되기 전까지 실행 계약이 아니다. 권장안을 기존 코드의 동작으로 설명하지 않는다.

`tasks.json`은 task ID·상태·의존성·acceptance의 단일 원본이다. ROADMAP은 단계 요약이며 별도의 완료 상태표를 만들지 않는다. 이 계획의 모든 신규 task는 planned, evidence는 비어 있다. `done`은 정확한 commit과 검사 결과, 계약/문서 변경, 제한이 있어야 한다. GitHub issue를 만들면 동일 task ID를 사용하고 동기화 담당자를 둔다.

운영·개인정보·지원 의무는 제안과 다르다. CN-003에서 범위/플랫폼/공급자/DB/라이선스/지원/예산의 실제 결정과 pending을 수집한다. 숫자를 측정 결과나 고객 약속으로 인용하지 않는다. risk R2/R3의 해당 통합 지점에서만 owner decision을 요구하고 무관한 작업은 계속한다.

optional task의 approved deferral은 done이 아니다. 의존 release의 scope와 edge를 owner 이유가 있는 변경으로 갱신하고 DAG를 다시 검사한다. write_scope 디렉터리는 최대 범위이며 실제 배정에서는 겹치지 않는 파일로 좁힌다.

## 첫 coordinator 프롬프트

```text
Cannon roadmap을 수행한다. AGENTS.md와 docs/strategy/README.md를 읽는다.
최신 main과 task/evidence 상태를 확인하고 이미 완료된 기능을 재구현하지 않는다.
CN-001을 우선 실행한다. CN-005와 CN-006은 별도 worktree에서 병렬 배정 가능하다.
선행 task와 승인 blocker가 해소된 작업만 구현하며, 큰 task는 동일 ID의
하위 작업 계약으로 나눈다. 구현자와 검증자는 독립적인 사례를 유지한다.
새 업무 정책/언어 의미/권한/비용을 조용히 정하지 않는다.
각 변경을 검증하고 계약과 handoff를 기록한다. 현재 Git 쓰기 허용과
저장소 규칙이 허용할 때 통합자만 검증한 커밋을 main에 fast-forward한다.
실패하거나 도구가 차단되면 사실을 기록하고 우회하지 않는다.
완료는 문서나 코드 양이 아니라 해당 task의 acceptance와 evidence로 판정한다.
```

아직 없는 미래 도구는 실행된 명령으로 기록하지 않는다. 현재 기준 명령은 `agent-execution.md`에 있다.

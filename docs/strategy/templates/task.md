# Task CN-XXX / subtask CN-XXX.a

Status: planned
Owner: unassigned
Reviewer: unassigned
Base commit: not captured
Risk: R0 / R1 / R2 / R3

## Outcome

사용자가 할 수 있게 되는 일과 이번 단위의 검증 가능한 결과를 적는다. 계획/문서/구현/연결/검증/출시 중 어느 종류인지 표시한다.

## Context pack

필독 계약 경로, 현재 타입/API, 고정 fixture, 관련 최근 변경, 현재 limitation을 기입한다. 전체 대화나 저장소를 무조건 복사하지 않는다.

## Dependencies and decisions

선행 task의 commit/evidence, 필요한 owner decision, 아직 모르는 것, 현재 task와 무관해서 계속 가능한 작업을 구분한다. planned를 ready로 오인하지 않는다.

## Write scope and interface

정확한 수정 경로, 새로 만들 경로, 건드리지 않을 경로, 공유 타입/포맷의 담당자를 적는다. 상위 tasks.json의 디렉터리 범위는 최대 경계이며 이 작업에서는 더 좁힌다. public interface 변경은 별도 합의 후 소비자를 갱신한다.

## Acceptance before implementation

입력/예상 결과/오류/원래 사례/한도/취소/복구를 기입한다. 새 구현의 결과로 expected를 계산하지 않는다. baseline expectation을 바꿔야 하면 이전 값과 이유·승인을 별도 항목으로 남긴다.

## Implementation outline and non-goals

핵심 단계, 가장 작은 E2E, 이번에 하지 않는 기능, 기존 evaluator/계약을 재사용하는 경로를 적는다. 연구 결과·측정·가정은 구분한다.

## Verification

정확한 명령과 환경, 독립 fixture, 실패·복구 시험, 필요한 실제 client/adapter, local과 remote 검증을 적는다. mock 성공과 실제 통합을 같은 결과로 기록하지 않는다.

## Budget and stop conditions

문맥/도구/비용 budget, 동시 수정 금지 경로, 반복 실패의 재검토 조건, 외부 효과/새 권한/의미 변경/보안 판정 시 중단 범위를 적는다. 끝내지 못한 경우 상태를 blocked 또는 in_review로 남긴다.

## Evidence and delivery

실제 commit, command, environment, result, exit code, logs/artifact, review 주체, 원격 main 상태, limitations. 미실행 검사는 not-run이다. future gate를 통과로 표시하지 않는다.

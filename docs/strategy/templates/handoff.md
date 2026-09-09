# Handoff: CN-XXX

## Exact state

Base SHA:
Candidate SHA:
Remote main SHA actually read:
Task status:
Working tree changes not committed:

## What changed

사용자 관점 결과, 변경한 파일, 기존 계약 유지 여부, 별도 의미/정답 기준/권한/의존성 변화를 적는다.

## What was verified

| Command or procedure | Environment / fixture | Observed result | Evidence |
|---|---|---|---|
| 실제 수행한 검사만 | 버전과 입력 | exit code/관측 | 정확한 run 또는 artifact |

작성했지만 실행하지 않은 테스트는 이 표에 성공으로 적지 않는다. 독립 검토가 없었으면 없다고 적는다. CI가 다른 SHA를 검사했으면 그 차이를 적는다.

## What was delivered

로컬 후보, Git object, branch commit, main 반영, release artifact, 실제 배포를 구분한다. 원격 main 재조회가 없으면 확인 완료라고 쓰지 않는다.

## Unknowns and blockers

미구현, 미검증, 알려진 결함, unsupported 환경, tool 권한/연결 부재, 보안 판정 차단을 구분한다. 완성을 약속하거나 이전 과거 검사를 현재 근거로 사용하지 않는다.

## Recovery and next smallest action

실패했을 때 원상복구/새 수정/재검사 방법과 다음 작업자가 실행할 하나의 최소 단위를 적는다. 자동 background 작업이 없는 경우 나중에 전달하겠다고 쓰지 않는다.

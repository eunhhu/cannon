# 이 전략 전달의 범위

2026-09-09. 이 커밋은 로드맵·출시/운영 계획·에이전트 작업표·템플릿을 전달한다. 실행 엔진, 테스트 기대값, 의존성, CI workflow, 저장소 권한은 바꾸지 않는다.

상세 `architecture.md`, `language-runtime.md`, `decisions.md` 내용을 묶어 기록하려던 요청 한 건은 도구의 보안 상태 판정에서 차단되었다. 해당 내용을 다른 경로/인코딩/도구로 다시 전달하지 않았다. 세 경로에는 미전달 상태를 알리는 짧은 안내만 둔다. 상세 설계가 전달된 것처럼 간주하면 안 된다.

사용 가능한 계획의 진입점은 ROADMAP, tasks.json, agent-execution, quality-release, security-operations다. 이들은 별도의 작업/출시/운영 문서로 기록되었다. 현재 동작의 기준은 기존 spec/architecture.md, spec/semantics.md, docs/native-* 계약이다.

작업표의 45개 task는 모두 미래 작업이다. 최초 DAG/JSON 구조 검사는 수행했지만, 지속적인 작업표 검증 도구의 구현은 CN-006에 남아 있다. 상세 설계 보충과 owner 결정 기록은 CN-002/003/013/015/024/028의 계약 작업에서 현재 실행 규칙과 합법적인 전달 가능 범위를 확인하여 진행한다. 미전달 문서의 내용을 이미 승인·적용된 계약으로 추정하지 않는다.

이 계획은 production 인증이나 전체 구현 완료를 뜻하지 않는다. 최종 delivery 보고에는 실제 main SHA와 정확한 CI 결과, 이 누락 범위를 함께 기록해야 한다.

import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { SnackService } from './snack.service';

describe('SnackService.locations', () => {
  let service: SnackService;
  let httpMock: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [SnackService, provideHttpClient(), provideHttpClientTesting()],
    });
    service = TestBed.inject(SnackService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => httpMock.verify());

  it('omits q when no term is given（筛选栏/初次加载返回全部）', () => {
    service.locations().subscribe();
    const request = httpMock.expectOne('/api/locations');
    expect(request.request.method).toBe('GET');
    expect(request.request.params.has('q')).toBe(false);
    request.flush(['厨房/冰箱']);
  });

  it('sends the trimmed term as q（自动补全按已输入字符过滤）', () => {
    service.locations('  客厅  ').subscribe();
    const request = httpMock.expectOne(
      (candidate) =>
        candidate.url === '/api/locations' && candidate.params.get('q') === '客厅',
    );
    expect(request.request.method).toBe('GET');
    request.flush(['客厅/零食柜']);
  });

  it('omits q when the term is blank', () => {
    service.locations('   ').subscribe();
    const request = httpMock.expectOne('/api/locations');
    expect(request.request.params.has('q')).toBe(false);
    request.flush([]);
  });
});
